//! Parser + sampler for Minecraft Bedrock `.animation.json` clips.

use std::collections::HashMap;

use serde::Deserialize;

use crate::molang::{Context, Expr};
use crate::BedrockError;

/// A set of animations keyed by their identifier (e.g. `animation.humanoid.idle`).
#[derive(Debug, Clone, Default)]
pub struct AnimationSet {
    pub clips: HashMap<String, AnimationClip>,
}

#[derive(Debug, Clone)]
pub struct AnimationClip {
    pub length: f32,
    pub looping: bool,
    pub bones: HashMap<String, BoneChannels>,
}

#[derive(Debug, Clone, Default)]
pub struct BoneChannels {
    pub rotation: Option<Channel>,
    pub position: Option<Channel>,
    pub scale: Option<Channel>,
}

/// A single animated property (3 components), either constant or keyframed.
#[derive(Debug, Clone)]
pub enum Channel {
    Constant([Expr; 3]),
    Keyframes(Vec<Keyframe>),
}

#[derive(Debug, Clone)]
pub struct Keyframe {
    pub time: f32,
    pub value: [Expr; 3],
}

/// Sampled transform delta for one bone at a given time.
#[derive(Debug, Clone, Copy)]
pub struct BonePose {
    /// Additive rotation on top of the bind pose, in degrees.
    pub rotation: [f32; 3],
    /// Position offset in model pixels.
    pub position: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for BonePose {
    fn default() -> Self {
        Self {
            rotation: [0.0; 3],
            position: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

// --- Raw JSON shapes ---

#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    animations: HashMap<String, RawAnimation>,
}

#[derive(Deserialize)]
struct RawAnimation {
    #[serde(default)]
    animation_length: Option<f32>,
    #[serde(default, rename = "loop")]
    looping: serde_json::Value,
    #[serde(default)]
    bones: HashMap<String, RawBone>,
}

#[derive(Deserialize)]
struct RawBone {
    #[serde(default)]
    rotation: Option<serde_json::Value>,
    #[serde(default)]
    position: Option<serde_json::Value>,
    #[serde(default)]
    scale: Option<serde_json::Value>,
}

impl AnimationSet {
    pub fn from_json_bytes(bytes: &[u8]) -> Result<AnimationSet, BedrockError> {
        let file: RawFile = serde_json::from_slice(bytes)?;
        let mut clips = HashMap::new();
        for (name, raw) in file.animations {
            clips.insert(name, clip_from_raw(raw));
        }
        Ok(AnimationSet { clips })
    }

    pub fn get(&self, name: &str) -> Option<&AnimationClip> {
        self.clips.get(name)
    }
}

fn clip_from_raw(raw: RawAnimation) -> AnimationClip {
    let looping = match raw.looping {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::String(ref s) => s == "true" || s == "hold_on_last_frame",
        _ => false,
    };
    let mut bones = HashMap::new();
    let mut max_kf_time = 0.0f32;
    for (name, rb) in raw.bones {
        let rotation = rb.rotation.as_ref().map(parse_channel);
        let position = rb.position.as_ref().map(parse_channel);
        let scale = rb.scale.as_ref().map(parse_channel);
        for ch in [&rotation, &position, &scale].into_iter().flatten() {
            if let Channel::Keyframes(kfs) = ch {
                for kf in kfs {
                    max_kf_time = max_kf_time.max(kf.time);
                }
            }
        }
        bones.insert(
            name,
            BoneChannels {
                rotation,
                position,
                scale,
            },
        );
    }
    let length = raw
        .animation_length
        .filter(|l| *l > 0.0)
        .unwrap_or_else(|| max_kf_time.max(0.001));
    AnimationClip {
        length,
        looping,
        bones,
    }
}

/// Parse a channel value: a scalar, a `[x,y,z]` array, or a keyframe map
/// `{ "0.0": [..], "1.0": {...} }`.
fn parse_channel(value: &serde_json::Value) -> Channel {
    match value {
        serde_json::Value::Object(map) => {
            // Keyframe map: keys are timestamps.
            let mut kfs: Vec<Keyframe> = map
                .iter()
                .filter_map(|(k, v)| {
                    let time = k.parse::<f32>().ok()?;
                    Some(Keyframe {
                        time,
                        value: parse_vec3(v),
                    })
                })
                .collect();
            kfs.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
            if kfs.is_empty() {
                Channel::Constant([Expr::Const(0.0), Expr::Const(0.0), Expr::Const(0.0)])
            } else {
                Channel::Keyframes(kfs)
            }
        }
        _ => Channel::Constant(parse_vec3(value)),
    }
}

/// Parse a value that may be a single scalar, a 3-array, or a keyframe object
/// with a `pre`/`post` value into three [`Expr`] components.
fn parse_vec3(value: &serde_json::Value) -> [Expr; 3] {
    match value {
        serde_json::Value::Array(arr) if arr.len() >= 3 => [
            Expr::from_json(&arr[0]),
            Expr::from_json(&arr[1]),
            Expr::from_json(&arr[2]),
        ],
        serde_json::Value::Object(map) => {
            // Keyframe with pre/post; prefer `post`, else `pre`.
            if let Some(inner) = map.get("post").or_else(|| map.get("pre")) {
                parse_vec3(inner)
            } else {
                [Expr::Const(0.0), Expr::Const(0.0), Expr::Const(0.0)]
            }
        }
        serde_json::Value::Number(_) | serde_json::Value::String(_) => {
            let e = Expr::from_json(value);
            [e.clone(), e.clone(), e]
        }
        _ => [Expr::Const(0.0), Expr::Const(0.0), Expr::Const(0.0)],
    }
}

impl AnimationClip {
    /// Wrap/clamp `time` (seconds) according to loop mode.
    pub fn local_time(&self, time: f32) -> f32 {
        if self.looping {
            time.rem_euclid(self.length)
        } else {
            time.min(self.length)
        }
    }

    /// Sample all animated bones at `time` (seconds since the clip started).
    pub fn sample(&self, time: f32, life_time: f32) -> HashMap<String, BonePose> {
        let anim_time = self.local_time(time);
        let ctx = Context {
            anim_time,
            life_time,
        };
        let mut out = HashMap::with_capacity(self.bones.len());
        for (name, channels) in &self.bones {
            let rotation = channels
                .rotation
                .as_ref()
                .map(|c| eval_channel(c, anim_time, &ctx))
                .unwrap_or([0.0; 3]);
            let position = channels
                .position
                .as_ref()
                .map(|c| eval_channel(c, anim_time, &ctx))
                .unwrap_or([0.0; 3]);
            let scale = channels
                .scale
                .as_ref()
                .map(|c| eval_channel(c, anim_time, &ctx))
                .unwrap_or([1.0; 3]);
            out.insert(
                name.clone(),
                BonePose {
                    rotation,
                    position,
                    scale,
                },
            );
        }
        out
    }
}

fn eval_channel(channel: &Channel, anim_time: f32, ctx: &Context) -> [f32; 3] {
    match channel {
        Channel::Constant(v) => [v[0].eval(ctx), v[1].eval(ctx), v[2].eval(ctx)],
        Channel::Keyframes(kfs) => {
            if kfs.is_empty() {
                return [0.0; 3];
            }
            if anim_time <= kfs[0].time {
                return eval_vec3(&kfs[0].value, ctx);
            }
            if anim_time >= kfs[kfs.len() - 1].time {
                return eval_vec3(&kfs[kfs.len() - 1].value, ctx);
            }
            for w in kfs.windows(2) {
                let a = &w[0];
                let b = &w[1];
                if anim_time >= a.time && anim_time <= b.time {
                    let span = (b.time - a.time).max(1e-6);
                    let t = (anim_time - a.time) / span;
                    let av = eval_vec3(&a.value, ctx);
                    let bv = eval_vec3(&b.value, ctx);
                    return [
                        av[0] + (bv[0] - av[0]) * t,
                        av[1] + (bv[1] - av[1]) * t,
                        av[2] + (bv[2] - av[2]) * t,
                    ];
                }
            }
            eval_vec3(&kfs[kfs.len() - 1].value, ctx)
        }
    }
}

fn eval_vec3(v: &[Expr; 3], ctx: &Context) -> [f32; 3] {
    [v[0].eval(ctx), v[1].eval(ctx), v[2].eval(ctx)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_samples_molang_rotation() {
        let json = r#"
        {
          "format_version": "1.8.0",
          "animations": {
            "animation.test.idle": {
              "loop": true,
              "animation_length": 2.0,
              "bones": {
                "rightArm": {
                  "rotation": ["math.cos(query.anim_time * 38.17) * 5", 0, 0]
                }
              }
            }
          }
        }"#;
        let set = AnimationSet::from_json_bytes(json.as_bytes()).unwrap();
        let clip = set.get("animation.test.idle").unwrap();
        assert!(clip.looping);
        assert!((clip.length - 2.0).abs() < 1e-4);
        let pose = clip.sample(0.0, 0.0);
        let arm = pose.get("rightArm").unwrap();
        // cos(0)=1 -> 5 degrees.
        assert!((arm.rotation[0] - 5.0).abs() < 1e-3);
    }

    #[test]
    fn keyframe_interpolation() {
        let json = r#"
        {
          "animations": {
            "animation.test.walk": {
              "animation_length": 1.0,
              "bones": {
                "leftLeg": {
                  "rotation": { "0.0": [0, 0, 0], "1.0": [40, 0, 0] }
                }
              }
            }
          }
        }"#;
        let set = AnimationSet::from_json_bytes(json.as_bytes()).unwrap();
        let clip = set.get("animation.test.walk").unwrap();
        let pose = clip.sample(0.5, 0.0);
        let leg = pose.get("leftLeg").unwrap();
        assert!((leg.rotation[0] - 20.0).abs() < 1e-3);
    }
}
