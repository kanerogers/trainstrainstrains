use common::{
    anyhow::{self, format_err as err, Context},
    glam::Vec3,
    hecs, log,
};
use components::{GLTFAsset, GLTFModel, Info, Material, Primitive, Texture, Vertex};
use gltf::Glb;
use image::codecs::png::PngDecoder;
use itertools::izip;
use std::{
    collections::HashMap,
    sync::{
        mpsc::{Receiver, SyncSender, TryRecvError},
        Arc,
    },
};

fn import_material(primitive: &gltf::Primitive<'_>, blob: &[u8]) -> anyhow::Result<Material> {
    let material = primitive.material();
    let pbr = material.pbr_metallic_roughness();
    let base_colour_factor = pbr.base_color_factor().into();

    let normal_texture = import_texture(material.normal_texture(), blob)
        .map_err(|e| log::warn!("Unable to import normal texture: {e:?}"))
        .ok();

    let base_colour_texture = import_texture(pbr.base_color_texture(), blob)
        .map_err(|e| log::warn!("Unable to import base colour texture: {e:?}"))
        .ok();

    let metallic_roughness_ao_texture = import_texture(pbr.metallic_roughness_texture(), blob)
        .map_err(|e| log::error!("Unable to import metallic roughness AO texture: {e:?}"))
        .ok();

    let emissive_texture = import_texture(material.emissive_texture(), blob)
        .map_err(|e| log::error!("Unable to import emissive texture: {e:?}"))
        .ok();

    Ok(Material {
        base_colour_texture,
        base_colour_factor,
        normal_texture,
        metallic_roughness_ao_texture,
        emissive_texture,
    })
}

fn import_texture<'a, T>(normal_texture: Option<T>, blob: &[u8]) -> anyhow::Result<Texture>
where
    T: AsRef<gltf::Texture<'a>>,
{
    let texture = normal_texture
        .as_ref()
        .ok_or_else(|| err!("Texture does not exist"))?
        .as_ref();

    let view = match texture.source().source() {
        gltf::image::Source::View {
            view,
            mime_type: "image/png",
        } => Ok(view),
        gltf::image::Source::View { mime_type, .. } => Err(err!("Invalid mime_type {mime_type}")),
        gltf::image::Source::Uri { .. } => Err(err!("Importing images by URI is not supported")),
    }?;
    let start = view.offset();
    let end = view.offset() + view.length();

    let image_bytes = blob
        .get(start..end)
        .ok_or_else(|| err!("Unable to read from blob with range {start}..{end}"))?;
    let decoder = PngDecoder::new(image_bytes)?;
    let image = image::DynamicImage::from_decoder(decoder)?;
    let image = image.into_rgba8();

    Ok(Texture {
        dimensions: image.dimensions().into(),
        data: image.to_vec(),
    })
}

pub enum AssetLoadState {
    Loading,
    Failed(String),
    Loaded(GLTFModel),
}

#[derive(Debug)]
pub struct AssetLoader {
    threadpool: futures_executor::ThreadPool,
    jobs: thunderdome::Arena<AssetLoadJob>,
    cache: HashMap<String, GLTFModel>,
}

type AssetResult = anyhow::Result<GLTFModel>;

pub struct AssetLoadToken {
    _inner: thunderdome::Index,
}

#[derive(Debug)]
struct AssetLoadJob {
    _inner: Receiver<AssetResult>,
}

impl AssetLoadJob {
    pub fn check(&self) -> AssetLoadState {
        match self._inner.try_recv() {
            Ok(asset_result) => match asset_result {
                Ok(a) => AssetLoadState::Loaded(a),
                Err(e) => AssetLoadState::Failed(format!("{e:?}")),
            },
            Err(TryRecvError::Empty) => AssetLoadState::Loading,
            Err(TryRecvError::Disconnected) => AssetLoadState::Failed(
                "The channel was disconnected. You probably loaded this asset already!".into(),
            ),
        }
    }
}

impl AssetLoader {
    pub fn load_assets(&mut self, world: &mut hecs::World) {
        log::debug!("Checking for assets to load..");
        let mut command_buffer = hecs::CommandBuffer::new();

        for (_, (asset, info)) in world.query::<(&GLTFAsset, &Info)>().iter() {
            log::debug!("Asset {asset:?} wants to be loaded by {info:?}");
        }

        // Check if there are any assets that are not yet imported
        for (entity, asset_to_import) in world
            .query::<&GLTFAsset>()
            .without::<hecs::Or<&GLTFModel, &AssetLoadToken>>()
            .iter()
        {
            log::info!("Requesting load of {}", &asset_to_import.name);
            if let Some(asset) = self.cache.get(&asset_to_import.name).cloned() {
                log::info!(
                    "{} is already in the cache; returning",
                    &asset_to_import.name
                );
                command_buffer.insert_one(entity, asset);
                continue;
            }

            let token = self.load(&asset_to_import.name);
            command_buffer.insert_one(entity, token);
        }

        // Check on the status of any tokens
        for (entity, (token, asset_to_import)) in
            world.query::<(&AssetLoadToken, &GLTFAsset)>().iter()
        {
            match self.check(token) {
                AssetLoadState::Loading => continue,
                AssetLoadState::Failed(e) => {
                    log::error!("Asset failed to load: {e:?}");
                    command_buffer.remove::<(AssetLoadToken, GLTFAsset)>(entity);
                }
                AssetLoadState::Loaded(asset) => {
                    log::info!("Successfully imported asset!");
                    self.cache.insert(asset_to_import.name.clone(), asset.clone());
                    command_buffer.remove_one::<AssetLoadToken>(entity);
                    command_buffer.insert_one(entity, asset);
                }
            }
        }

        command_buffer.run_on(world);
    }

    pub fn new() -> Self {
        let threadpool = futures_executor::ThreadPool::new().unwrap();
        Self {
            threadpool,
            jobs: Default::default(),
            cache: Default::default(),
        }
    }

    fn check(&self, token: &AssetLoadToken) -> AssetLoadState {
        self.jobs.get(token._inner).unwrap().check()
    }

    fn load<S: Into<String>>(&mut self, asset_name: S) -> AssetLoadToken {
        // oneshot channel
        let (sender, receiver) = std::sync::mpsc::sync_channel(0);
        self.threadpool
            .spawn_ok(load_and_insert(asset_name.into(), sender));
        let index = self.jobs.insert(AssetLoadJob { _inner: receiver });
        AssetLoadToken { _inner: index }
    }
}

async fn load_and_insert(asset_name: String, sender: SyncSender<AssetResult>) {
    let asset_result = load(asset_name);
    sender
        .send(asset_result)
        .unwrap_or_else(|e| log::error!("Failed to send asset: {e:?}"));
}

fn load(asset_name: String) -> anyhow::Result<GLTFModel> {
    #[cfg(debug_assertions)]
    let assets_folder = format!("{}/../assets", env!("CARGO_MANIFEST_DIR"));

    #[cfg(not(debug_assertions))]
    let assets_folder = "./assets";

    let asset_path = format!("{assets_folder}/{asset_name}");
    let file = std::fs::read(&asset_path).context(asset_path)?;
    let glb = Glb::from_slice(&file)?;
    let root = gltf::json::Root::from_slice(&glb.json)?;
    let document = gltf::Document::from_json(root)?;
    let blob = glb.bin.ok_or_else(|| err!("No binary found in glTF"))?;
    let node = document
        .nodes()
        .next()
        .ok_or_else(|| err!("No nodes found in glTF"))?;

    let mut primitives = Vec::new();
    let mesh = node.mesh().ok_or_else(|| err!("Node has no mesh"))?;

    for primitive in mesh.primitives() {
        let vertices = import_vertices(&primitive, &blob)?;
        let indices = import_indices(&primitive, &blob)?;

        let material = import_material(&primitive, &blob)?;

        primitives.push(Primitive {
            vertices,
            indices,
            material,
        });
    }

    return Ok(GLTFModel {
        primitives: Arc::new(primitives),
    });
}

fn import_vertices(primitive: &gltf::Primitive<'_>, blob: &[u8]) -> anyhow::Result<Vec<Vertex>> {
    let reader = primitive.reader(|_| Some(blob));
    let position_reader = reader
        .read_positions()
        .ok_or_else(|| err!("Primitive has no positions"))?;
    let normal_reader = reader
        .read_normals()
        .ok_or_else(|| err!("Primitive has no normals"))?;
    let uv_reader = reader
        .read_tex_coords(0)
        .ok_or_else(|| err!("Primitive has no UVs"))?
        .into_f32();
    let vertices = izip!(position_reader, normal_reader, uv_reader)
        .map(|(position, normal, uv)| Vertex {
            position: Vec3::from(position).extend(1.),
            normal: Vec3::from(normal).extend(1.),
            uv: uv.into(),
        })
        .collect();
    Ok(vertices)
}

fn import_indices(primitive: &gltf::Primitive<'_>, blob: &[u8]) -> anyhow::Result<Vec<u32>> {
    let reader = primitive.reader(|_| Some(blob));
    let indices = reader
        .read_indices()
        .ok_or_else(|| err!("Primitive has no indices"))?
        .into_u32()
        .collect();
    Ok(indices)
}

#[cfg(test)]
mod tests {
    use components::GLTFAsset;

    use super::*;

    #[test]
    fn loading_assets() {
        env_logger::init();
        let mut asset_loader = AssetLoader::new();
        let mut world = hecs::World::new();
        let entities_to_spawn = 16;
        for i in 0..entities_to_spawn {
            world.spawn((i, GLTFAsset::new("droid.glb")));
        }

        loop {
            asset_loader.load_assets(&mut world);

            if world
                .query_mut::<()>()
                .with::<(&GLTFAsset, &GLTFModel)>()
                .without::<&AssetLoadToken>()
                .into_iter()
                .count()
                == 16
            {
                break;
            }
        }

        let (_, model) = world
            .query_mut::<&GLTFModel>()
            .without::<&AssetLoadToken>()
            .into_iter()
            .next()
            .unwrap();

        let primitive = &model.primitives[0];
        assert_eq!(primitive.vertices.len(), 40455);

        let material = &primitive.material;
        assert!(material.base_colour_texture.is_some());
        assert!(material.normal_texture.is_some());
    }
}
