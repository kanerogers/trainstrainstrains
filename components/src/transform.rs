use std::ops::Mul;

use common::{
    glam::{Affine3A, Mat4, Quat, Vec3},
    rapier3d::na,
};

#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub position: Vec3,
    pub scale: Vec3,
    pub rotation: Quat,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Default::default(),
            scale: Vec3::ONE,
            rotation: Default::default(),
        }
    }
}

impl Transform {
    pub fn new(position: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self {
            position,
            scale,
            rotation,
        }
    }

    pub fn from_position<V: Into<Vec3>>(position: V) -> Self {
        Self {
            position: position.into(),
            ..Default::default()
        }
    }

    pub fn from_rotation_position(rotation: Quat, position: Vec3) -> Self {
        Self {
            rotation,
            position,
            ..Default::default()
        }
    }
}

impl Mul<&Transform> for &Transform {
    type Output = Transform;

    fn mul(self, rhs: &Transform) -> Self::Output {
        (Affine3A::from(self) * Affine3A::from(rhs)).into()
    }
}

impl Mul<Transform> for Transform {
    type Output = Transform;

    fn mul(self, rhs: Transform) -> Self::Output {
        &self * &rhs
    }
}

impl From<Affine3A> for Transform {
    fn from(value: Affine3A) -> Self {
        let (scale, rotation, position) = value.to_scale_rotation_translation();
        Transform {
            position,
            rotation,
            scale,
        }
    }
}

impl From<&Transform> for Affine3A {
    fn from(value: &Transform) -> Self {
        Affine3A::from_scale_rotation_translation(value.scale, value.rotation, value.position)
    }
}

impl From<&Transform> for Mat4 {
    fn from(value: &Transform) -> Self {
        Mat4::from_scale_rotation_translation(value.scale, value.rotation, value.position)
    }
}

impl From<&Transform> for na::Isometry3<f32> {
    fn from(value: &Transform) -> Self {
        na::Isometry::from_parts(
            value.position.to_array().into(),
            na::UnitQuaternion::from_quaternion(na::Quaternion::from_parts(
                value.rotation.w,
                value.rotation.xyz().to_array().into(),
            )),
        )
    }
}
