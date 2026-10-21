//! Recordings (`proposals/live-viewer.md` L4): a numbered PNG sequence of one view of a running
//! world, and a manifest that says how to draw it again. Each PNG is what `emerge-server render`
//! writes for that tick. Files go only under the server's `--journal-dir`, never where a caller says.

use crate::api::Engine;
use emerge_lang::ast::View;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct Recording {
    pub id: u64,
    world: String,
    view: View,
    views: Option<String>,
    every: u64,
    left: u64,
    cell: u32,
    pub dir: PathBuf,
    /// File, tick and revision of each frame written.
    frames: Vec<(String, u64, u64)>,
    last_tick: Option<u64>,
}

impl Recording {
    /// Starts recording `frames` frames of view `view` of world `world`, one every `every` ticks,
    /// into `root/recordings/<world>-r<id>/` (a directory that must not exist yet).
    pub fn start(engine: &Engine, root: &Path, id: u64, p: &Value) -> Result<Recording, String> {
        let world = p["world"].as_str().ok_or("`world` must be a string")?;
        let run = engine.run(world).ok_or(format!("no world `{world}`"))?;
        let views = p["views"].as_str().map(String::from);
        let view = crate::picture::pick_view(run, views.as_deref(), p["view"].as_str())?;
        let frames = p["frames"].as_u64().ok_or("`frames` must be a whole number")?;
        if frames == 0 || frames > 100_000 {
            return Err("`frames` must be 1 to 100_000".into());
        }
        let dir = root.join("recordings").join(format!("{world}-r{id}"));
        std::fs::create_dir_all(dir.parent().expect("a parent")).map_err(|e| e.to_string())?;
        std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Recording { id, world: world.to_string(), view, views, every: p["every"].as_u64().unwrap_or(1).max(1), left: frames, cell: p["cell"].as_u64().unwrap_or(0) as u32, dir, frames: Vec::new(), last_tick: None })
    }

    /// Writes a frame if one is due (the first at once, then every `every` ticks of the world).
    /// Returns false once the recording is done and its manifest written: after its last frame,
    /// or early if the world died out (it will not tick again).
    pub fn advance(&mut self, engine: &Engine) -> Result<bool, String> {
        let run = engine.run(&self.world).ok_or("the world is gone")?;
        if self.last_tick.is_none_or(|t| run.tick() >= t + self.every) {
            let (png, _, _) = crate::picture::png(run, &self.view, self.cell)?;
            let file = format!("frame-{:06}.png", self.frames.len() + 1);
            std::fs::write(self.dir.join(&file), png).map_err(|e| e.to_string())?;
            self.frames.push((file, run.tick(), run.revision()));
            self.last_tick = Some(run.tick());
            self.left -= 1;
        }
        let died = !run.alive() && self.last_tick == Some(run.tick());
        if self.left > 0 && !died {
            return Ok(true);
        }
        let spec = &run.spec;
        let manifest = json!({
            "world": self.world, "seed": spec.seed, "lineages": spec.lineages, "mutation": spec.mutation, "contract": spec.contract,
            "backend": run.device, "source": spec.source,
            "note": "the source at revision 0; frames at a later revision need the world's journal (`<world>.jsonl` in the server's --journal-dir)",
            "ended": if self.left == 0 { "complete".to_string() } else { format!("the world died out at tick {}", run.tick()) },
            "view": self.view.name, "views": self.views, "every": self.every, "cell": self.cell,
            "frames": self.frames.iter().map(|(f, t, r)| json!({"file": f, "tick": t, "revision": r})).collect::<Vec<_>>(),
        });
        std::fs::write(self.dir.join("manifest.json"), serde_json::to_string_pretty(&manifest).expect("json")).map_err(|e| e.to_string())?;
        Ok(false)
    }

    pub fn status(&self) -> Value {
        json!({"recording": self.id, "world": self.world, "view": self.view.name, "written": self.frames.len(), "left": self.left, "dir": self.dir.display().to_string()})
    }
}

/// A MOV made with `ffmpeg` from a finished recording's frames, `fps` frames per second.
pub fn mov(dir: &Path, fps: u32) -> Result<PathBuf, String> {
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).map_err(|e| format!("{}: {e}", dir.display()))?).map_err(|e| e.to_string())?;
    let n = manifest["frames"].as_array().map_or(0, Vec::len);
    if n == 0 {
        return Err("the recording has no frames".into());
    }
    let out = dir.join("recording.mov");
    // Frames can be any size: pad to even dimensions, which the encoder needs.
    let status = std::process::Command::new("ffmpeg")
        .args(["-loglevel", "error", "-nostdin", "-n", "-framerate", &fps.to_string(), "-i"])
        .arg(dir.join("frame-%06d.png"))
        .args(["-frames:v", &n.to_string(), "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&out)
        .status()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpeg failed ({status})"));
    }
    Ok(out)
}
