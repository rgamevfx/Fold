//! One pinned root supplies both exact-time image strips and package-owned probes.
use super::*;
use fold_foundation::Time;
use fold_media::Cancel;
use fold_project::{CommittedSnapshot, DocumentRef};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub source: DocumentRef,
    pub times: Vec<Time>,
    pub dimensions: [u32; 2],
    #[serde(default)]
    pub candidate: Option<u64>,
    #[serde(default)]
    pub generation: Option<u64>,
    #[serde(default)]
    pub project_epoch: Option<u64>,
    #[serde(default)]
    pub queries: Vec<Value>,
}
impl Observation {
    pub fn validate(&self, snapshot: &CommittedSnapshot) -> Result<(), String> {
        if self.times.is_empty() || self.times.len() > 12 {
            return Err("observe requires 1..12 exact times".into());
        }
        if self.queries.len() > 8 {
            return Err("at most eight evaluated queries per frame".into());
        }
        for q in &self.queries {
            if q.get("time").is_some() {
                return Err("observation supplies query time from times; omit query.time".into());
            }
            if q["mode"] != "evaluated" {
                return Err(
                    "observation queries require mode evaluated; time is supplied by observation"
                        .into(),
                );
            }
        }
        let [w, h] = self.dimensions;
        if w == 0
            || h == 0
            || w > 2048
            || h > 2048
            || u64::from(w) * u64::from(h) * self.times.len() as u64 > 2_000_000
        {
            return Err(
                "observation dimensions require 1..2048 and at most 2000000 total pixels".into(),
            );
        }
        let info = crate::packages::builtins().output_ref(snapshot, &self.source)?;
        let end = info.time(info.frames)?;
        if self.times.iter().any(|t| *t < Time::ZERO || *t >= end) {
            return Err("observation time outside half-open output range".into());
        }
        Ok(())
    }
    pub fn run(
        self,
        snapshot: &CommittedSnapshot,
        renderer: &mut crate::output::OutputRenderer,
        cancel: &Cancel,
        epoch: u64,
    ) -> Result<Value, String> {
        self.validate(snapshot)?;
        if self.project_epoch.is_some_and(|expected| expected != epoch) {
            return Err("project epoch conflict".into());
        }
        let [w, h] = self.dimensions;
        let columns = (self.times.len() as u32).min(4);
        let rows = (self.times.len() as u32).div_ceil(columns);
        let width = w * columns;
        let height = h * rows;
        let mut strip = vec![0u8; (width * height * 4) as usize];
        let registry = crate::packages::builtins();
        let document = snapshot
            .state()
            .documents
            .get(&self.source.document)
            .ok_or("missing source document")?;
        let environment = super::evidence::identity(snapshot, cancel)?;
        let mut frames = Vec::new();
        for (index, time) in self.times.iter().enumerate() {
            cancel.check()?;
            let began = std::time::Instant::now();
            let frame = renderer.evaluate(
                snapshot,
                &crate::media_workflow::SceneRequest {
                    source: self.source.clone(),
                    time: *time,
                    dimensions: self.dimensions,
                    region: None,
                    preview: None,
                },
                cancel,
            )?;
            if frame.dimensions() != self.dimensions {
                return Err("renderer returned unexpected dimensions".into());
            }
            let x = (index as u32 % columns) * w;
            let y = (index as u32 / columns) * h;
            for row in 0..h {
                let src = (row * w * 4) as usize;
                let dst = ((y + row) * width * 4 + x * 4) as usize;
                strip[dst..dst + (w * 4) as usize]
                    .copy_from_slice(&frame.rgba()[src..src + (w * 4) as usize]);
            }
            let mut measurements = Vec::new();
            for query in &self.queries {
                cancel.check()?;
                let mut q = query.clone();
                q["time"] = json!(time);
                measurements.push(match registry.query_cancellable(document, &q, cancel) {
                    Ok(value) => json!({"status":"evaluated","query":q,"data":value}),
                    Err(e) => json!({"status":"unavailable","query":q,"reason":e}),
                });
            }
            frames.push(
                json!({"index":index,"time":time,"rect":[x,y,w,h],"render_seconds":began.elapsed().as_secs_f64(),"measurements":measurements}),
            );
        }
        cancel.check()?;
        let metadata = json!({"kind":"observation","revision":snapshot.revision(),"project_epoch":epoch,"candidate":self.candidate,"generation":self.generation,"source":self.source,"frames":frames,"color":fold_platform::color::output(snapshot,self.source.document)?,"alpha":"over black","overlays":false,"ordering":"row-major","build":env!("CARGO_PKG_VERSION"),"environment":environment,"backend":renderer.backend(),"cost_scope":"render_seconds includes rendering and selected measurement queries; per-stage costs unavailable","verification":"sampled evidence only; no aesthetic or analytical guarantee"});
        let result = images::rgba_detail(width, height, &strip, metadata, width.max(height))?;
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            > 12 * 1024 * 1024
        {
            return Err(
                "observation exceeds 12 MiB response budget; reduce dimensions or queries".into(),
            );
        }
        Ok(result)
    }
}
