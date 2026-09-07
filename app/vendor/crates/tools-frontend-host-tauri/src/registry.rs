use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
#[derive(Clone)]
struct WindowRecord {
    app: String,
    project: String,
    session: String,
    generation: u32,
    source: Option<String>,
    transfer: Option<String>,
    panels: BTreeSet<String>,
}
#[derive(Clone)]
struct Transfer {
    source: String,
    destination: String,
    panels: BTreeSet<String>,
    status: String,
    deadline: Instant,
    layout: String,
}
#[derive(Clone, PartialEq, Eq)]
struct ReleaseReceipt {
    app: String,
    project: String,
    session: String,
    window: String,
    generation: u32,
}
#[derive(Default, Clone)]
pub(crate) struct Registry {
    windows: BTreeMap<String, WindowRecord>,
    transfers: BTreeMap<String, Transfer>,
    pub(crate) recovery: BTreeMap<String, BTreeSet<String>>,
    revision: u64,
    pub(crate) layouts: BTreeMap<String, String>,
    last_release: Option<ReleaseReceipt>,
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
    {
        return Err("Invalid bounded host identifier".into());
    }
    Ok(())
}
fn panels(v: &Value) -> Result<BTreeSet<String>, String> {
    let list = v["panel_ids"].as_array().ok_or("Missing panel IDs")?;
    if list.len() > 256 {
        return Err("Panel limit exceeded".into());
    }
    list.iter()
        .map(|x| {
            let id = x.as_str().ok_or("Invalid panel ID")?;
            identifier(id)?;
            Ok(id.to_owned())
        })
        .collect()
}
impl Registry {
    fn response(&self, label: &str) -> Result<Value, String> {
        let w = self.windows.get(label).ok_or("Unknown window")?;
        let desired = w
            .transfer
            .as_ref()
            .and_then(|t| self.transfers.get(t))
            .filter(|t| t.status == "pending")
            .map(|t| &t.panels)
            .unwrap_or(&w.panels);
        let layout = w
            .transfer
            .as_ref()
            .and_then(|t| self.transfers.get(t))
            .map(|t| t.layout.as_str())
            .or_else(|| {
                self.layouts
                    .get(&format!("{}/{}", w.app, w.project))
                    .map(String::as_str)
            });
        Ok(
            json!({"operation":"state","app_id":w.app,"project_id":w.project,"session_id":w.session,"window_id":label,"generation":w.generation,"source_id":w.source,"transfer_id":w.transfer,"panel_ids":desired,"owned_panel_ids":w.panels,"layout":layout,"ownership_revision":self.revision.to_string()}),
        )
    }
    fn transfer_response(&self, label: &str, id: &str) -> Result<Value, String> {
        let t = self.transfers.get(id).ok_or("Unknown transfer")?;
        let mut result = self.response(label)?;
        result["transfer_id"] = json!(id);
        result["destination_id"] = json!(t.destination);
        result["status"] = json!(t.status);
        Ok(result)
    }
    fn release_response(&self, label: &str, q: &Value) -> Option<Value> {
        let receipt = self.last_release.as_ref()?;
        if text(q, "operation") != "end_session"
            || receipt.app != text(q, "app_id")
            || receipt.project != text(q, "project_id")
            || receipt.session != text(q, "session_id")
            || receipt.window != label
            || text(q, "window_id") != label
            || q["generation"].as_u64() != Some(receipt.generation as u64)
        {
            return None;
        }
        let mut response = q.clone();
        response["status"] = json!("ended");
        Some(response)
    }
    /// Persist a candidate release while the caller holds the registry lock.
    /// Failed validation or persistence cannot invalidate the live identity.
    pub(crate) fn end_session(
        &mut self, label: &str, q: Value,
        persist: impl FnOnce(&Registry) -> Result<(), String>,
    ) -> Result<Value, String> {
        if text(&q, "operation") != "end_session" { return Err("Expected end_session".into()); }
        if let Some(response) = self.release_response(label, &q) {
            return Ok(response);
        }
        let mut candidate = self.clone();
        let response = candidate.exchange(label, q)?;
        persist(&candidate)?;
        *self = candidate;
        Ok(response)
    }
    pub(crate) fn exchange(&mut self, label: &str, q: Value) -> Result<Value, String> {
        identifier(label)?;
        identifier(text(&q, "app_id"))?;
        for t in self.transfers.values_mut() {
            if text(&q, "operation") != "end_session" && t.status == "pending" && Instant::now() >= t.deadline {
                t.status = "cancelled".into();
            }
        }
        if text(&q, "operation") == "bootstrap" {
            if !self.windows.contains_key(label) {
                if label != "main" {
                    return Err("Unregistered destination window".into());
                }
                identifier(text(&q, "project_id"))?;
                let mut ids = panels(&q)?;
                let key = format!("{}/{}", text(&q, "app_id"), text(&q, "project_id"));
                ids.extend(self.recovery.entry(key).or_default().iter().cloned());
                self.windows.insert(
                    label.into(),
                    WindowRecord {
                        app: text(&q, "app_id").into(),
                        project: text(&q, "project_id").into(),
                        session: uuid::Uuid::new_v4().to_string(),
                        generation: 1,
                        source: None,
                        transfer: None,
                        panels: ids,
                    },
                );
                self.revision += 1;
            }
            if self.windows[label].app != text(&q, "app_id") {
                return Err("Foreign application".into());
            }
            if label == "main" && self.windows[label].project != text(&q, "project_id") {
                return Err("Project switching requires a new window host session".into());
            }
            return self.response(label);
        }
        if let Some(response) = self.release_response(label, &q) {
            return Ok(response);
        }
        let w = self.windows.get(label).ok_or("Unknown window")?.clone();
        if text(&q, "app_id") != w.app
            || text(&q, "project_id") != w.project
            || text(&q, "session_id") != w.session
            || text(&q, "window_id") != label
            || q["generation"].as_u64() != Some(w.generation as u64)
        {
            return Err("Stale or foreign window identity".into());
        }
        match text(&q, "operation") {
            "state" => self.response(label),
            "end_session" => {
                if label != "main" { return Err("Only main can end the session".into()); }
                if self.windows.len() != 1 || self.transfers.values().any(|t| t.status == "pending") {
                    return Err("Secondary windows or pending transfers remain".into());
                }
                let layout = text(&q, "layout");
                if layout.len() > 65536 { return Err("Layout limit exceeded".into()); }
                let key = format!("{}/{}", w.app, w.project);
                self.recovery.insert(key.clone(), w.panels);
                if q["layout"].is_string() { self.layouts.insert(key, layout.into()); }
                let mut result = self.response(label)?;
                result["status"] = json!("ended");
                self.last_release = Some(ReleaseReceipt {
                    app: w.app.clone(),
                    project: w.project.clone(),
                    session: w.session.clone(),
                    window: label.into(),
                    generation: w.generation,
                });
                self.windows.remove(label);
                self.transfers.clear();
                self.revision += 1;
                Ok(result)
            }
            "detach" => {
                let ids = panels(&q)?;
                if ids.is_empty() || !ids.is_subset(&w.panels) {
                    return Err("Panel is not owned by this window".into());
                }
                if self.windows.len() >= 32 {
                    return Err("Window limit exceeded".into());
                }
                if self.transfers.values().any(|t| {
                    t.status == "pending" && t.source == label && !t.panels.is_disjoint(&ids)
                }) {
                    return Err("Panel transfer already pending".into());
                }
                let layout = text(&q, "layout");
                if layout.len() > 65536 {
                    return Err("Layout limit exceeded".into());
                }
                if !layout.is_empty() {
                    self.layouts
                        .insert(format!("{}/{}", w.app, w.project), layout.into());
                }
                let id = uuid::Uuid::new_v4().to_string();
                let destination = format!("host-{}", uuid::Uuid::new_v4());
                self.windows.insert(
                    destination.clone(),
                    WindowRecord {
                        panels: BTreeSet::new(),
                        source: Some(label.into()),
                        transfer: Some(id.clone()),
                        ..w.clone()
                    },
                );
                self.revision += 1;
                self.transfers.insert(
                    id.clone(),
                    Transfer {
                        source: label.into(),
                        destination,
                        panels: ids,
                        status: "pending".into(),
                        deadline: Instant::now() + Duration::from_secs(15),
                        layout: layout.into(),
                    },
                );
                self.recovery
                    .entry(format!("{}/{}", w.app, w.project))
                    .or_default()
                    .extend(w.panels);
                self.transfer_response(label, &id)
            }
            "status" | "cancel" | "ready" => {
                let id = text(&q, "transfer_id");
                let t = self.transfers.get_mut(id).ok_or("Unknown transfer")?;
                let op = text(&q, "operation");
                if (op == "ready" && t.destination != label) || (op != "ready" && t.source != label)
                {
                    return Err("Foreign transfer participant".into());
                }
                if op == "cancel" && t.status == "pending" {
                    t.status = "cancelled".into()
                }
                if op == "ready" && t.status == "pending" {
                    let source = self
                        .windows
                        .get_mut(&t.source)
                        .ok_or("Source window closed")?;
                    for panel in &t.panels {
                        source.panels.remove(panel);
                    }
                    self.windows
                        .get_mut(&t.destination)
                        .ok_or("Destination closed")?
                        .panels
                        .extend(t.panels.clone());
                    t.status = "committed".into();
                    self.revision += 1;
                }
                self.transfer_response(label, id)
            }
            "return" => {
                if label == "main" {
                    return Err("Main window cannot return to itself".into());
                }
                let result = self.response(label)?;
                self.close(label);
                Ok(result)
            }
            _ => Err("Unsupported host operation".into()),
        }
    }
    pub(crate) fn close(&mut self, label: &str) {
        let Some(w) = self.windows.remove(label) else {
            return;
        };
        self.revision += 1;
        let eligible = |candidate: &WindowRecord| {
            candidate.session == w.session
                && candidate.transfer.as_ref().is_none_or(|id| {
                    self.transfers.get(id).is_some_and(|t| t.status == "committed")
                })
        };
        let target = w
            .source
            .as_ref()
            .filter(|id| {
                self.windows
                    .get(*id)
                    .is_some_and(eligible)
            })
            .cloned()
            .or_else(|| {
                self.windows
                    .iter()
                    .find(|(_, v)| eligible(v))
                    .map(|(id, _)| id.clone())
            });
        if let Some(target) = target {
            self.windows
                .get_mut(&target)
                .unwrap()
                .panels
                .extend(w.panels.clone());
        }
        self.recovery
            .entry(format!("{}/{}", w.app, w.project))
            .or_default()
            .extend(w.panels);
        for t in self.transfers.values_mut() {
            if t.destination == label {
                t.status = if t.status == "committed" {
                    "returned"
                } else {
                    "cancelled"
                }
                .into()
            }
            if t.source == label && t.status == "pending" {
                t.status = "cancelled".into()
            }
        }
    }
    pub(crate) fn cancelled_destinations(&self) -> Vec<String> {
        self.transfers
            .values()
            .filter(|t| t.status == "cancelled")
            .map(|t| t.destination.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn start() -> (Registry, Value) {
        let mut registry = Registry::default();
        let state=registry.exchange("main",json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":["editor"]})).unwrap();
        (registry, state)
    }
    fn op(state: &Value, operation: &str) -> Value {
        let mut r = state.clone();
        r["operation"] = json!(operation);
        r
    }
    fn detach(r: &mut Registry, s: &Value) -> Value {
        let mut q = op(s, "detach");
        q["panel_ids"] = json!(["editor"]);
        r.exchange("main", q).unwrap()
    }
    #[test]
    fn end_session_failed_persistence_preserves_identity_and_recovery() {
        let (mut r, s) = start();
        let mut q = op(&s, "end_session"); q["layout"] = json!("new-layout");
        let before = r.recovery.clone();
        assert_eq!(r.end_session("main", q.clone(), |candidate| {
            assert!(!candidate.windows.contains_key("main"));
            assert_eq!(candidate.layouts["atelier/p1"], "new-layout");
            Err("disk full".into())
        }).unwrap_err(), "disk full");
        assert_eq!(r.recovery, before); assert!(r.layouts.is_empty());
        assert_eq!(r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"], json!(["editor"]));
        let mut retried_persistence = false;
        r.end_session("main", q, |_| {
            retried_persistence = true;
            Ok(())
        })
        .unwrap();
        assert!(retried_persistence);
        assert!(r.exchange("main", op(&s, "state")).is_err());
    }
    #[test]
    fn exact_release_retry_skips_persistence_and_preserves_replacement() {
        let (mut registry, first) = start();
        let first_end = op(&first, "end_session");
        registry
            .end_session("main", first_end.clone(), |_| Ok(()))
            .unwrap();
        let replacement = registry.exchange("main", json!({
            "operation":"bootstrap", "app_id":"atelier", "project_id":"p2", "panel_ids":["viewport"]
        })).unwrap();
        let mut persisted = false;
        let replay = registry
            .end_session("main", first_end.clone(), |_| {
                persisted = true;
                Err("receipt replay must not persist".into())
            })
            .unwrap();
        assert_eq!(replay["status"], "ended");
        assert!(!persisted);
        assert_eq!(
            registry
                .exchange("main", op(&replacement, "state"))
                .unwrap()["owned_panel_ids"],
            json!(["viewport"])
        );
        let mut altered = first_end;
        altered["generation"] = json!(2);
        assert!(registry.end_session("main", altered, |_| Ok(())).is_err());
    }
    #[test]
    fn end_session_releases_main_and_reconciles_only_the_latest_exact_release() {
        let (mut r, s) = start();
        let mut end = op(&s, "end_session"); end["layout"] = json!("saved-layout");
        assert_eq!(r.exchange("main", end).unwrap()["status"], "ended");
        assert!(r.exchange("main", op(&s, "state")).is_err());
        let next = r.exchange("main", json!({"operation":"bootstrap","app_id":"atelier","project_id":"p2","panel_ids":["viewport"]})).unwrap();
        assert_ne!(next["session_id"], s["session_id"]);
        let altered_fields = [
            ("app_id", json!("other")),
            ("project_id", json!("other")),
            ("session_id", json!("other")),
            ("window_id", json!("other")),
            ("generation", json!(2)),
        ];
        for (field, value) in altered_fields {
            let mut altered = op(&s, "end_session");
            altered[field] = value;
            assert!(r.exchange("main", altered).is_err());
            assert_eq!(
                r.exchange("main", op(&next, "state")).unwrap()["owned_panel_ids"],
                json!(["viewport"])
            );
        }
        assert!(r.exchange("other", op(&s, "end_session")).is_err());
        assert_eq!(
            r.exchange("main", op(&next, "state")).unwrap()["owned_panel_ids"],
            json!(["viewport"])
        );
        assert!(r.exchange("main", op(&s, "state")).is_err());
        assert_eq!(
            r.exchange("main", op(&next, "state")).unwrap()["owned_panel_ids"],
            json!(["viewport"])
        );
        assert_eq!(
            r.exchange("main", op(&s, "end_session")).unwrap()["status"],
            "ended"
        );
        assert_eq!(
            r.exchange("main", op(&next, "state")).unwrap()["project_id"],
            "p2"
        );
        let mut altered = op(&s, "end_session");
        altered["generation"] = json!(2);
        assert!(r.exchange("main", altered).is_err());
        let next_end = op(&next, "end_session");
        r.exchange("main", next_end.clone()).unwrap();
        assert!(r.exchange("main", op(&s, "end_session")).is_err());
        assert_eq!(r.exchange("main", next_end).unwrap()["status"], "ended");
        let restored = r.exchange("main", json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":[]})).unwrap();
        assert_eq!(restored["panel_ids"], json!(["editor"]));
        assert_eq!(restored["layout"], "saved-layout");
    }
    #[test]
    fn end_session_refuses_pending_and_committed_children_without_closing_them() {
        let (mut r, s) = start(); let t = detach(&mut r, &s);
        let label = t["destination_id"].as_str().unwrap();
        assert!(r.exchange("main", op(&s, "end_session")).is_err());
        let child = r.exchange(label, json!({"operation":"bootstrap","app_id":"atelier"})).unwrap();
        assert!(r.exchange(label, op(&child, "end_session")).is_err());
        r.exchange(label, op(&child, "ready")).unwrap();
        assert!(r.exchange("main", op(&s, "end_session")).is_err());
        assert_eq!(r.exchange(label, op(&child, "state")).unwrap()["owned_panel_ids"], json!(["editor"]));
        r.close(label); r.exchange("main", op(&s, "end_session")).unwrap();
    }
    #[test]
    fn acknowledgement_moves_ownership_once_and_close_returns_it() {
        let (mut r, s) = start();
        let t = detach(&mut r, &s);
        let child = t["destination_id"].as_str().unwrap();
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!(["editor"])
        );
        let boot = r
            .exchange(child, json!({"operation":"bootstrap","app_id":"atelier"}))
            .unwrap();
        assert_eq!(
            r.exchange(child, op(&boot, "ready")).unwrap()["status"],
            "committed"
        );
        assert_eq!(
            r.exchange(child, op(&boot, "ready")).unwrap()["status"],
            "committed"
        );
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!([])
        );
        r.close(child);
        r.close(child);
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!(["editor"])
        );
    }
    #[test]
    fn stale_foreign_and_cancelled_acknowledgements_do_not_move_panels() {
        let (mut r, s) = start();
        let t = detach(&mut r, &s);
        let child = t["destination_id"].as_str().unwrap();
        let boot = r
            .exchange(child, json!({"operation":"bootstrap","app_id":"atelier"}))
            .unwrap();
        let mut stale = op(&boot, "ready");
        stale["generation"] = json!(99);
        assert!(r.exchange(child, stale).is_err());
        let mut foreign = op(&boot, "ready");
        foreign["project_id"] = json!("other");
        assert!(r.exchange(child, foreign).is_err());
        let mut cancel = op(&s, "cancel");
        cancel["transfer_id"] = t["transfer_id"].clone();
        r.exchange("main", cancel).unwrap();
        assert_eq!(
            r.exchange(child, op(&boot, "ready")).unwrap()["status"],
            "cancelled"
        );
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!(["editor"])
        );
    }
    #[test]
    fn rejects_unregistered_window_and_malformed_identifiers() {
        let (mut r, _) = start();
        assert!(r
            .exchange(
                "https://evil",
                json!({"operation":"bootstrap","app_id":"atelier"})
            )
            .is_err());
        assert!(Registry::default()
            .exchange(
                "main",
                json!({"operation":"bootstrap","app_id":"../evil","project_id":"p","panel_ids":[]})
            )
            .is_err());
    }
    #[test]
    fn timeout_retains_source_and_rejects_late_ack() {
        let (mut r, s) = start();
        let t = detach(&mut r, &s);
        let id = t["transfer_id"].as_str().unwrap();
        r.transfers.get_mut(id).unwrap().deadline = std::time::Instant::now();
        let child = t["destination_id"].as_str().unwrap();
        let boot = r
            .exchange(child, json!({"operation":"bootstrap","app_id":"atelier"}))
            .unwrap();
        assert_eq!(
            r.exchange(child, op(&boot, "ready")).unwrap()["status"],
            "cancelled"
        );
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!(["editor"])
        );
    }
    #[test]
    fn closing_destination_before_readiness_keeps_source_ownership() {
        let (mut r, s) = start();
        let t = detach(&mut r, &s);
        r.close(t["destination_id"].as_str().unwrap());
        let mut status = op(&s, "status");
        status["transfer_id"] = t["transfer_id"].clone();
        assert_eq!(r.exchange("main", status).unwrap()["status"], "cancelled");
        assert_eq!(
            r.exchange("main", op(&s, "state")).unwrap()["owned_panel_ids"],
            json!(["editor"])
        );
    }
    #[test]
    fn duplicate_detach_and_foreign_source_status_are_rejected() {
        let (mut r, s) = start();
        let t = detach(&mut r, &s);
        let mut duplicate = op(&s, "detach");
        duplicate["panel_ids"] = json!(["editor"]);
        assert!(r.exchange("main", duplicate).is_err());
        let child = t["destination_id"].as_str().unwrap();
        let boot = r
            .exchange(child, json!({"operation":"bootstrap","app_id":"atelier"}))
            .unwrap();
        assert!(r.exchange(child, op(&boot, "status")).is_err());
    }
    #[test]
    fn restart_recovers_orphan_panel_ids_and_layout_intent() {
        let (mut r, s) = start();
        let mut request = op(&s, "detach");
        request["panel_ids"] = json!(["editor"]);
        request["layout"] = json!("captured-layout");
        r.exchange("main", request).unwrap();
        let mut restarted = Registry {
            recovery: r.recovery,
            layouts: r.layouts,
            ..Registry::default()
        };
        let restored=restarted.exchange("main",json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":["viewport"]})).unwrap();
        assert_eq!(restored["owned_panel_ids"], json!(["editor", "viewport"]));
        assert_eq!(restored["layout"], "captured-layout");
        assert_ne!(restored["session_id"], s["session_id"]);
    }
    #[test]
    fn source_reload_nested_detach_and_main_close_preserve_authoritative_ownership() {
        let mut r=Registry::default();
        let main=r.exchange("main",json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":["editor","viewport"]})).unwrap();
        let t1=detach(&mut r,&main);let child_label=t1["destination_id"].as_str().unwrap();
        let child=r.exchange(child_label,json!({"operation":"bootstrap","app_id":"atelier"})).unwrap();r.exchange(child_label,op(&child,"ready")).unwrap();
        let reloaded=r.exchange("main",json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":["editor","viewport"]})).unwrap();
        assert_eq!(reloaded["owned_panel_ids"],json!(["viewport"]));
        let mut request=op(&child,"detach");request["panel_ids"]=json!(["editor"]);let t2=r.exchange(child_label,request).unwrap();let grandchild_label=t2["destination_id"].as_str().unwrap();
        let grandchild=r.exchange(grandchild_label,json!({"operation":"bootstrap","app_id":"atelier"})).unwrap();r.exchange(grandchild_label,op(&grandchild,"ready")).unwrap();
        assert_eq!(r.exchange(child_label,op(&child,"state")).unwrap()["owned_panel_ids"],json!([]));
        r.close("main");r.close(grandchild_label);
        let remaining=r.exchange(child_label,op(&child,"state")).unwrap();assert_eq!(remaining["owned_panel_ids"],json!(["editor","viewport"]));
        assert!(remaining["ownership_revision"].as_str().unwrap().parse::<u64>().unwrap()>reloaded["ownership_revision"].as_str().unwrap().parse::<u64>().unwrap());
    }
    #[test]
    fn main_close_does_not_return_panels_to_a_cancelled_pending_destination() {
        let mut r=Registry::default();let main=r.exchange("main",json!({"operation":"bootstrap","app_id":"atelier","project_id":"p1","panel_ids":["editor","viewport"]})).unwrap();
        let pending=detach(&mut r,&main);let old=pending["destination_id"].as_str().unwrap();let record=r.windows.remove(old).unwrap();r.windows.insert("host-0".into(),record);r.transfers.get_mut(pending["transfer_id"].as_str().unwrap()).unwrap().destination="host-0".into();
        let mut request=op(&main,"detach");request["panel_ids"]=json!(["viewport"]);let accepted=r.exchange("main",request).unwrap();let label=accepted["destination_id"].as_str().unwrap();
        let child=r.exchange(label,json!({"operation":"bootstrap","app_id":"atelier"})).unwrap();r.exchange(label,op(&child,"ready")).unwrap();
        r.close("main");let remaining=r.exchange(label,op(&child,"state")).unwrap();assert_eq!(remaining["owned_panel_ids"],json!(["editor","viewport"]));assert!(r.windows["host-0"].panels.is_empty());
    }
}
