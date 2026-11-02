use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct InputOrder {
    last: u64,
    pending: BTreeMap<u64, Vec<u8>>,
    deadline: Option<Instant>,
}
impl InputOrder {
    pub fn push(&mut self, seq: u64, bytes: Vec<u8>) -> Result<Vec<Vec<u8>>, ()> {
        if self.expired()
            || seq <= self.last
            || seq - self.last > 32
            || self.pending.contains_key(&seq)
        {
            return Err(());
        }
        self.pending.insert(seq, bytes);
        let mut ready = Vec::new();
        while let Some(bytes) = self.pending.remove(&(self.last + 1)) {
            self.last += 1;
            ready.push(bytes);
        }
        if self.pending.is_empty() {
            self.deadline = None;
        } else if self.deadline.is_none() {
            self.deadline = Some(Instant::now() + Duration::from_secs(2));
        }
        Ok(ready)
    }
    pub fn expired(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }
}
pub(crate) fn valid_input(v: &Value, layout: u64) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    let allowed: &[&str] = match v["kind"].as_str() {
        Some("move") => &["kind", "display", "layout", "x", "y"],
        Some("button") => &["kind", "display", "layout", "x", "y", "button", "down"],
        Some("wheel") => &[
            "kind",
            "display",
            "layout",
            "x",
            "y",
            "vertical",
            "horizontal",
        ],
        Some("key") => {
            return o
                .keys()
                .all(|k| ["kind", "vk", "down", "repeat"].contains(&k.as_str()))
                && v["vk"].as_u64().is_some_and(|n| n <= 255)
                && v["down"].is_boolean()
                && (v.get("repeat").is_none() || v["repeat"].is_boolean());
        }
        Some("text") => return o.len() == 2 && v["text"].as_str().is_some_and(|s| s.len() <= 768),
        _ => return false,
    };
    o.len() == allowed.len()
        && o.keys().all(|k| allowed.contains(&k.as_str()))
        && v["display"] == 1
        && v["layout"].as_u64() == Some(layout)
        && ["x", "y"].iter().all(|k| {
            v[k].as_f64()
                .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
        })
        && (v["kind"] != "button"
            || (v["down"].is_boolean() && matches!(v["button"].as_str(), Some("left" | "right"))))
        && (v["kind"] != "wheel"
            || ["vertical", "horizontal"]
                .iter()
                .all(|k| v[k].as_i64().is_some_and(|n| (-1200..=1200).contains(&n))))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reorder_and_replay() {
        let mut q = InputOrder::default();
        assert!(q.push(2, vec![2]).unwrap().is_empty());
        assert_eq!(q.push(1, vec![1]).unwrap(), vec![vec![1], vec![2]]);
        assert!(q.push(2, vec![]).is_err());
        assert!(q.push(35, vec![]).is_err());
    }
    #[test]
    fn gaps_expire_even_under_traffic() {
        let mut q = InputOrder::default();
        q.push(2, vec![]).unwrap();
        q.deadline = Some(Instant::now() - Duration::from_secs(1));
        assert!(q.push(1, vec![]).is_err());
    }
    #[test]
    fn geometry_and_fields_fail_closed() {
        let mut v = json!({"kind":"button","display":1,"layout":5,"x":0.2,"y":0.8,"button":"left","down":true});
        assert!(valid_input(&v, 5));
        assert!(!valid_input(&v, 6));
        v["x"] = json!(1.1);
        assert!(!valid_input(&v, 5));
        v["x"] = json!(0.2);
        v["extra"] = json!(true);
        assert!(!valid_input(&v, 5));
        assert!(!valid_input(
            &json!({"kind":"key","vk":65,"down":"true"}),
            5
        ));
    }
}
