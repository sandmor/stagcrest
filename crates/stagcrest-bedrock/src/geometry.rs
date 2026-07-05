//! Parser for Minecraft Bedrock `minecraft:geometry` model JSON (format 1.12+).

use serde::Deserialize;

use crate::BedrockError;

/// A parsed Bedrock geometry (one entry of `minecraft:geometry`).
#[derive(Debug, Clone)]
pub struct Geometry {
    pub identifier: String,
    pub texture_width: u32,
    pub texture_height: u32,
    pub bones: Vec<Bone>,
}

#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: Option<String>,
    /// Bone pivot in model pixels (16 px = 1 block), Bedrock model space.
    pub pivot: [f32; 3],
    /// Bind-pose rotation in degrees.
    pub rotation: [f32; 3],
    pub mirror: bool,
    pub cubes: Vec<Cube>,
}

#[derive(Debug, Clone)]
pub struct Cube {
    pub origin: [f32; 3],
    pub size: [f32; 3],
    pub uv: CubeUv,
    pub inflate: f32,
    /// Per-cube pivot for cube rotation (defaults to origin-based when absent).
    pub pivot: Option<[f32; 3]>,
    pub rotation: [f32; 3],
    pub mirror: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum CubeUv {
    /// Classic box unwrap starting at `[u, v]`.
    Box { origin: [f32; 2] },
    /// Explicit per-face UV rectangles.
    PerFace(FaceUvSet),
}

#[derive(Debug, Clone, Default)]
pub struct FaceUvSet {
    pub north: Option<FaceUv>,
    pub south: Option<FaceUv>,
    pub east: Option<FaceUv>,
    pub west: Option<FaceUv>,
    pub up: Option<FaceUv>,
    pub down: Option<FaceUv>,
}

#[derive(Debug, Clone, Copy)]
pub struct FaceUv {
    pub uv: [f32; 2],
    pub uv_size: [f32; 2],
}

// --- Raw JSON shapes ---

#[derive(Deserialize)]
struct RawFile {
    #[serde(rename = "minecraft:geometry")]
    geometry: Option<Vec<RawGeometry>>,
}

#[derive(Deserialize)]
struct RawGeometry {
    description: RawDescription,
    #[serde(default)]
    bones: Vec<RawBone>,
}

#[derive(Deserialize)]
struct RawDescription {
    #[serde(default)]
    identifier: String,
    #[serde(default = "default_tex_size")]
    texture_width: u32,
    #[serde(default = "default_tex_size")]
    texture_height: u32,
}

fn default_tex_size() -> u32 {
    64
}

#[derive(Deserialize)]
struct RawBone {
    name: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    pivot: Option<[f32; 3]>,
    #[serde(default)]
    rotation: Option<[f32; 3]>,
    #[serde(default)]
    mirror: bool,
    #[serde(default)]
    cubes: Vec<RawCube>,
}

#[derive(Deserialize)]
struct RawCube {
    origin: [f32; 3],
    size: [f32; 3],
    #[serde(default)]
    uv: serde_json::Value,
    #[serde(default)]
    inflate: f32,
    #[serde(default)]
    pivot: Option<[f32; 3]>,
    #[serde(default)]
    rotation: Option<[f32; 3]>,
    #[serde(default)]
    mirror: Option<bool>,
}

#[derive(Deserialize)]
struct RawFaceUv {
    uv: [f32; 2],
    #[serde(default)]
    uv_size: Option<[f32; 2]>,
}

impl Geometry {
    /// Parse the first geometry entry from a `.geo.json` byte buffer.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Geometry, BedrockError> {
        let file: RawFile = serde_json::from_slice(bytes)?;
        let mut geos = file
            .geometry
            .ok_or_else(|| BedrockError::MissingField("minecraft:geometry"))?;
        if geos.is_empty() {
            return Err(BedrockError::MissingField("minecraft:geometry[0]"));
        }
        Ok(Self::from_raw(geos.remove(0)))
    }

    fn from_raw(raw: RawGeometry) -> Geometry {
        let bones = raw
            .bones
            .into_iter()
            .map(|b| Bone {
                name: b.name,
                parent: b.parent,
                pivot: b.pivot.unwrap_or([0.0, 0.0, 0.0]),
                rotation: b.rotation.unwrap_or([0.0, 0.0, 0.0]),
                mirror: b.mirror,
                cubes: b.cubes.into_iter().map(cube_from_raw).collect(),
            })
            .collect();
        Geometry {
            identifier: raw.description.identifier,
            texture_width: raw.description.texture_width.max(1),
            texture_height: raw.description.texture_height.max(1),
            bones,
        }
    }
}

fn cube_from_raw(raw: RawCube) -> Cube {
    Cube {
        origin: raw.origin,
        size: raw.size,
        uv: parse_cube_uv(&raw.uv),
        inflate: raw.inflate,
        pivot: raw.pivot,
        rotation: raw.rotation.unwrap_or([0.0, 0.0, 0.0]),
        mirror: raw.mirror,
    }
}

fn parse_cube_uv(value: &serde_json::Value) -> CubeUv {
    match value {
        serde_json::Value::Array(arr) if arr.len() >= 2 => {
            let u = arr[0].as_f64().unwrap_or(0.0) as f32;
            let v = arr[1].as_f64().unwrap_or(0.0) as f32;
            CubeUv::Box { origin: [u, v] }
        }
        serde_json::Value::Object(_) => {
            let parse_face = |name: &str| -> Option<FaceUv> {
                let raw: RawFaceUv = serde_json::from_value(value.get(name)?.clone()).ok()?;
                Some(FaceUv {
                    uv: raw.uv,
                    uv_size: raw.uv_size.unwrap_or([0.0, 0.0]),
                })
            };
            CubeUv::PerFace(FaceUvSet {
                north: parse_face("north"),
                south: parse_face("south"),
                east: parse_face("east"),
                west: parse_face("west"),
                up: parse_face("up"),
                down: parse_face("down"),
            })
        }
        _ => CubeUv::Box { origin: [0.0, 0.0] },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "format_version": "1.12.0",
      "minecraft:geometry": [
        {
          "description": {
            "identifier": "geometry.test",
            "texture_width": 64,
            "texture_height": 64
          },
          "bones": [
            { "name": "body", "pivot": [0, 24, 0], "cubes": [
              { "origin": [-4, 12, -2], "size": [8, 12, 4], "uv": [16, 16] }
            ]},
            { "name": "head", "parent": "body", "pivot": [0, 24, 0], "cubes": [
              { "origin": [-4, 24, -4], "size": [8, 8, 8], "uv": [0, 0] }
            ]}
          ]
        }
      ]
    }
    "#;

    #[test]
    fn parses_bones_and_cubes() {
        let geo = Geometry::from_json_bytes(SAMPLE.as_bytes()).unwrap();
        assert_eq!(geo.identifier, "geometry.test");
        assert_eq!(geo.texture_width, 64);
        assert_eq!(geo.bones.len(), 2);
        assert_eq!(geo.bones[0].name, "body");
        assert_eq!(geo.bones[1].parent.as_deref(), Some("body"));
        assert_eq!(geo.bones[1].cubes[0].size, [8.0, 8.0, 8.0]);
        assert!(matches!(geo.bones[0].cubes[0].uv, CubeUv::Box { .. }));
    }

    #[test]
    fn parses_per_face_uv() {
        let json = r#"
        { "minecraft:geometry": [ { "description": {}, "bones": [
          { "name": "b", "cubes": [ { "origin": [0,0,0], "size": [1,1,1], "uv": {
            "north": {"uv": [0,0], "uv_size": [1,1]},
            "up": {"uv": [2,2], "uv_size": [1,1]}
          }}]}
        ]}]}"#;
        let geo = Geometry::from_json_bytes(json.as_bytes()).unwrap();
        match &geo.bones[0].cubes[0].uv {
            CubeUv::PerFace(set) => {
                assert!(set.north.is_some());
                assert!(set.up.is_some());
                assert!(set.south.is_none());
            }
            _ => panic!("expected per-face uv"),
        }
    }
}
