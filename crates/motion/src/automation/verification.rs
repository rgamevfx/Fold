//! Sampled outcome checks over the same native evaluator as preview/export.
//! These checks measure geometry; they cannot certify rendered readability.
use super::observation;
use crate::Motion;
use fold_foundation::{ObjectId, Time};
use fold_media::Cancel;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct Request {
    mode: String,
    check: String,
    times: Vec<Time>,
    #[serde(default)]
    target: Value,
    #[serde(default)]
    tolerance: Option<f64>,
    #[serde(default)]
    expected: Option<[f64; 2]>,
    #[serde(default)]
    relative_to: Option<Value>,
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    control: Option<ObjectId>,
    #[serde(default)]
    values: Option<[crate::fields::Datum; 2]>,
}
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Request)).expect("verification schema")
}
pub fn verify(m: &Motion, q: &Value, cancel: &Cancel) -> Result<Value, String> {
    let q: Request = fold_platform::authoring::decode(q)?;
    if q.mode != "verify"
        || ![
            "movement",
            "direction",
            "displacement",
            "periodicity",
            "nonempty",
            "relative_position",
            "control_response",
        ]
        .contains(&q.check.as_str())
    {
        return Err(
            "check must be movement, direction, displacement, periodicity, nonempty, relative_position or control_response".into(),
        );
    }
    if q.times.len() < 2 || q.times.len() > 64 || q.times.windows(2).any(|t| t[0] >= t[1]) {
        return Err("verify requires 2..64 strictly increasing exact times".into());
    }
    let tolerance = q.tolerance.unwrap_or(0.01);
    if !tolerance.is_finite() || tolerance < 0. {
        return Err("tolerance must be finite and nonnegative".into());
    }
    if q.check == "periodicity" && q.times.len() < 3 {
        return Err("periodicity requires endpoints and an intermediate sample; also run movement to reject static output".into());
    }
    let mut probe = if q.target.is_null() {
        json!({})
    } else {
        q.target
    };
    for key in probe.as_object().ok_or("target must be an object")?.keys() {
        if !["object", "node", "output"].contains(&key.as_str()) {
            return Err(format!("unsupported target field {key}"));
        }
    }
    probe["mode"] = json!("evaluated");
    probe["fields"] = json!(["bounds", "counts", "elements"]);
    probe["limit"] = json!(200);
    if q.check == "control_response" {
        let id = q
            .control
            .ok_or("control_response requires published control node ID")?;
        let node = m.graph.node(id)?;
        if node.kind != "fold.motion.published_control" {
            return Err("control must identify a Published Control node".into());
        }
        let settings = node.settings::<crate::nodes::interface::ControlSettings>()?;
        let values = q
            .values
            .ok_or("control_response requires two tagged values")?;
        for value in &values {
            value.validate()?;
            if value.kind() != settings.value_type {
                return Err(
                    "control_response values must match the control's declared type".into(),
                );
            }
        }
        if values[0] == values[1] {
            return Err("control_response requires different values".into());
        }
        let mut variants = [m.clone(), m.clone()];
        for (variant, value) in variants.iter_mut().zip(&values) {
            variant.controls.insert(settings.id, value.clone());
        }
        let mut pairs = vec![];
        let mut changed = false;
        let mut incomplete = false;
        for time in &q.times {
            probe["time"] = json!(time);
            let mut pair = vec![];
            for variant in &variants {
                pair.push(observation::evaluate(variant, &probe, cancel)?);
            }
            incomplete |= pair.iter().any(|sample| {
                sample["truncated"] == true
                    || sample["status"] == "inconclusive"
                    || sample["elements"]
                        .as_array()
                        .is_some_and(|elements| elements.iter().any(|e| e["image_effects"] == true))
            });
            changed |= pair[0] != pair[1];
            pairs.push(json!({"time":time,"variants":pair}));
        }
        return Ok(
            json!({"check":"control_response","status":if incomplete {"inconclusive"} else if changed {"pass"} else {"fail"},"control":id,"values":values,"samples":pairs,"evidence":"sampled native evaluated output under two private control overrides; no project mutation","limitation":"detects exact changes in bounded geometry/paint or uniform field evidence; does not establish the intended magnitude, readability or pixel visibility. Truncated, varying-field and effect evidence is inconclusive; use displacement/direction and rendered comparisons for intended response."}),
        );
    }
    let mut samples = vec![];
    for time in &q.times {
        cancel.check()?;
        probe["time"] = json!(time);
        samples.push(observation::evaluate(m, &probe, cancel)?);
    }
    let mut result = json!({"check":q.check,"evidence":"sampled native geometry; no analytical or pixel-visibility guarantee","times":q.times,"tolerance":tolerance,"samples":samples,"status":"inconclusive"});
    let samples = result["samples"].as_array().unwrap();
    if q.check == "nonempty" {
        result["status"] = json!(if samples.iter().all(|s| s["empty"] == false) {
            "pass"
        } else {
            "fail"
        });
        return Ok(result);
    }
    let bounds: Option<Vec<[f64; 4]>> = samples
        .iter()
        .map(|s| serde_json::from_value(s["bounds"].clone()).ok())
        .collect();
    let Some(bounds) = bounds else {
        result["reason"] = json!("target has no measurable geometry bounds at one or more times");
        return Ok(result);
    };
    if q.check == "relative_position" {
        let mut other = q
            .relative_to
            .ok_or("relative_position requires relative_to target")?;
        for key in other
            .as_object()
            .ok_or("relative_to must be an object")?
            .keys()
        {
            if !["object", "node", "output"].contains(&key.as_str()) {
                return Err(format!("unsupported relative_to field {key}"));
            }
        }
        let relation = q
            .relation
            .as_deref()
            .ok_or("relative_position requires relation: left, right, above, below or offset")?;
        if !["left", "right", "above", "below", "offset"].contains(&relation) {
            return Err("unsupported relation; use left, right, above, below or offset".into());
        }
        if relation == "offset" && q.expected.is_none() {
            return Err("offset requires expected [dx,dy] between outline-bounds centers".into());
        }
        other["mode"] = json!("evaluated");
        other["fields"] = json!(["bounds"]);
        let mut comparisons = vec![];
        let mut passed = true;
        for (time, a) in q.times.iter().zip(&bounds) {
            other["time"] = json!(time);
            let sample = observation::evaluate(m, &other, cancel)?;
            let Ok(b) = serde_json::from_value::<[f64; 4]>(sample["bounds"].clone()) else {
                result["reason"] = json!("relative target has no measurable geometry bounds");
                result["failing_time"] = json!(time);
                return Ok(result);
            };
            let offset = [
                (a[0] + a[2] - b[0] - b[2]) / 2.,
                (a[1] + a[3] - b[1] - b[3]) / 2.,
            ];
            let matches = match relation {
                "left" => a[2] <= b[0] + tolerance,
                "right" => a[0] >= b[2] - tolerance,
                "above" => a[3] <= b[1] + tolerance,
                "below" => a[1] >= b[3] - tolerance,
                "offset" => {
                    let e = q.expected.unwrap();
                    (offset[0] - e[0]).hypot(offset[1] - e[1]) <= tolerance
                }
                _ => unreachable!(),
            };
            passed &= matches;
            comparisons.push(json!({"time":time,"target_bounds":a,"relative_bounds":b,"center_offset":offset,"pass":matches}));
        }
        result["status"] = json!(if passed { "pass" } else { "fail" });
        result["comparisons"] = json!(comparisons);
        result["relation"] = json!(relation);
        result["limitation"] = json!(
            "outline bounds only, positive Y downward; stroke/effects/clipping and pixel visibility are excluded"
        );
        return Ok(result);
    }
    let centers: Vec<[f64; 2]> = bounds
        .iter()
        .map(|b| [(b[0] + b[2]) / 2., (b[1] + b[3]) / 2.])
        .collect();
    let delta = [
        centers.last().unwrap()[0] - centers[0][0],
        centers.last().unwrap()[1] - centers[0][1],
    ];
    let distance = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let passed = match q.check.as_str() {
        "movement" => centers.iter().any(|c| distance(*c, centers[0]) > tolerance),
        "displacement" => {
            let expected = q.expected.ok_or("displacement requires expected [dx,dy]")?;
            distance(delta, expected) <= tolerance
        }
        "direction" => {
            let direction = q.expected.ok_or("direction requires expected [dx,dy]")?;
            let length = direction[0].hypot(direction[1]);
            if length == 0. || !length.is_finite() {
                return Err("direction must be finite and nonzero".into());
            }
            centers.windows(2).all(|p| {
                ((p[1][0] - p[0][0]) * direction[0] + (p[1][1] - p[0][1]) * direction[1]) / length
                    > tolerance
            })
        }
        "periodicity" => {
            let first = &samples[0];
            let last = samples.last().unwrap();
            if first["truncated"] == true || last["truncated"] == true {
                result["reason"] = json!("periodicity evidence truncated; select a smaller target");
                return Ok(result);
            }
            if [first, last].iter().any(|sample| {
                sample["elements"].as_array().is_some_and(|elements| {
                    elements
                        .iter()
                        .any(|element| element["image_effects"] == true)
                })
            }) {
                result["reason"] =
                    json!("image effects or masks require rendered periodicity evidence");
                return Ok(result);
            }
            if first["counts"] != last["counts"] {
                false
            } else {
                let a = first["elements"]
                    .as_array()
                    .ok_or("missing element evidence")?;
                let b = last["elements"]
                    .as_array()
                    .ok_or("missing element evidence")?;
                a.len() == b.len()
                    && a.iter().zip(b).all(|(a, b)| {
                        a["id"] == b["id"]
                            && a["kind"] == b["kind"]
                            && a["path_signature"] == b["path_signature"]
                            && a["fill"] == b["fill"]
                            && a["stroke"] == b["stroke"]
                            && a["transform"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .zip(b["transform"].as_array().unwrap())
                                .all(|(a, b)| {
                                    (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= tolerance
                                })
                            && (a["opacity"].as_f64().unwrap() - b["opacity"].as_f64().unwrap())
                                .abs()
                                <= tolerance
                    })
            }
        }
        _ => unreachable!(),
    };
    result["status"] = json!(if passed { "pass" } else { "fail" });
    result["measured_displacement"] = json!(delta);
    result["centers"] = json!(centers);
    result["limitation"] = json!(
        "movement/direction measure outline-bounds centers; periodicity compares element identities, exact diagnostic path signatures and paint, and transforms/opacity within tolerance; image effects/masks are inconclusive. Diagnostic hashes are not an analytical proof. Pair with same-snapshot rendered observation."
    );
    Ok(result)
}
