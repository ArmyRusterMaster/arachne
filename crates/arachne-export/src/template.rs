//! Рендеринг `OutputTemplate` в JSON нужного формата из плоских `Record`.
//!
//! Подстановки:
//! - `{{name}}` — первое значение поля `name` (учитывая индексы `name[0]`);
//! - `{{name[*]}}` — все значения поля списком;
//! - секция `__each__: "repeat_name"` — повторяется по блокам `repeat_name`:
//!   внутри `{{nested_name}}` собирает значения `nested_name[block.j]` списком.
//!   Плоские поля `flat_name[block]` подставляются по индексу блока.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::Record;

/// Записи страницы, сгруппированные по имени поля (без индексов).
pub type FieldIndex = BTreeMap<String, Vec<String>>;

/// Вложенные группы: имя → для каждого блока список значений `name[i.j]`.
pub type NestedGroups = BTreeMap<String, Vec<Vec<String>>>;

/// Сгруппировать записи страницы по имени поля (`quote_text[0]` → `quote_text`).
pub fn group_fields(records: &[Record]) -> FieldIndex {
    let mut idx: FieldIndex = BTreeMap::new();
    for r in records {
        let base = r.field.split('[').next().unwrap_or(&r.field).to_string();
        idx.entry(base).or_default().push(r.value.clone());
    }
    idx
}

/// Построить вложенные группы из записей с путём `name[i.j]`.
pub fn group_nested(records: &[Record]) -> NestedGroups {
    let mut out: NestedGroups = BTreeMap::new();
    for r in records {
        let Some(open) = r.field.find('[') else {
            continue;
        };
        let name = &r.field[..open];
        let path = r.field[open + 1..].trim_end_matches(']');
        let mut parts = path.split('.');
        let Ok(block) = parts.next().unwrap_or("").parse::<usize>() else {
            continue;
        };
        // Только вложенные (второй индекс присутствует).
        if parts.next().is_some() {
            let slots = out.entry(name.to_string()).or_default();
            if slots.len() <= block {
                slots.resize(block + 1, Vec::new());
            }
            slots[block].push(r.value.clone());
        }
    }
    out
}

/// Отрендерить шаблон в JSON.
pub fn render(tpl: &Value, idx: &FieldIndex, nested: &NestedGroups) -> Result<Value, String> {
    render_value(tpl, idx, nested)
}

fn render_value(tpl: &Value, idx: &FieldIndex, nested: &NestedGroups) -> Result<Value, String> {
    match tpl {
        Value::String(s) => Ok(render_string(s, idx, nested)),
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|v| render_value(v, idx, nested))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Value::Object(o) => {
            // __each__: повтор по блокам repeat-поля.
            if let Some(Value::String(repeat_name)) = o.get("__each__") {
                let blocks = idx.get(repeat_name).map(Vec::len).unwrap_or(0);
                let mut items = Vec::with_capacity(blocks);
                for b in 0..blocks {
                    items.push(render_each_item(o, idx, nested, b)?);
                }
                return Ok(Value::Array(items));
            }
            let mut m = Map::new();
            for (k, v) in o {
                m.insert(k.clone(), render_value(v, idx, nested)?);
            }
            Ok(Value::Object(m))
        }
        other => Ok(other.clone()),
    }
}

/// Рендер одного элемента `__each__` для блока `block`.
fn render_each_item(
    tpl_obj: &Map<String, Value>,
    idx: &FieldIndex,
    nested: &NestedGroups,
    block: usize,
) -> Result<Value, String> {
    let mut m = Map::new();
    for (k, v) in tpl_obj {
        if k == "__each__" {
            continue;
        }
        m.insert(k.clone(), render_each_value(v, idx, nested, block)?);
    }
    Ok(Value::Object(m))
}

fn render_each_value(
    tpl: &Value,
    idx: &FieldIndex,
    nested: &NestedGroups,
    block: usize,
) -> Result<Value, String> {
    match tpl {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
                let name = &trimmed[2..trimmed.len() - 2];
                // 1) Вложенные поля повторяющегося блока (name[i.j]).
                if let Some(blocks) = nested.get(name) {
                    if let Some(vals) = blocks.get(block) {
                        return serde_json::to_value(vals).map_err(|e| e.to_string());
                    }
                    return Ok(json!([]));
                }
                // 2) Плоские поля с индексом первого уровня (name[block]).
                if let Some(vals) = idx.get(name) {
                    if let Some(v) = vals.get(block) {
                        return Ok(json!(v.clone()));
                    }
                    return Ok(json!(null));
                }
                return Ok(json!(null));
            }
            // Внутри строки: подставляем значения вложенного/плоского поля блока.
            let mut out = s.to_string();
            for (name, blocks) in nested {
                let placeholder = format!("{{{{{name}}}}}");
                if out.contains(&placeholder) {
                    let joined = blocks.get(block).map(|v| v.join(", ")).unwrap_or_default();
                    out = out.replace(&placeholder, &joined);
                }
            }
            for (name, vals) in idx {
                let placeholder = format!("{{{{{name}}}}}");
                if out.contains(&placeholder) {
                    let val = vals.get(block).cloned().unwrap_or_default();
                    out = out.replace(&placeholder, &val);
                }
            }
            Ok(Value::String(out))
        }
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|v| render_each_value(v, idx, nested, block))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, v) in o {
                m.insert(k.clone(), render_each_value(v, idx, nested, block)?);
            }
            Ok(Value::Object(m))
        }
        other => Ok(other.clone()),
    }
}

/// Подстановка `{{name}}` / `{{name[*]}}` в строке (плоские поля).
fn substitute_flat(s: &str, idx: &FieldIndex) -> String {
    let mut out = s.to_string();
    for (name, vals) in idx {
        let first = vals.first().map(String::as_str).unwrap_or("");
        out = out.replace(&format!("{{{{{name}}}}}"), first);
        let all = serde_json::to_string(vals).unwrap_or_else(|_| "[]".into());
        out = out.replace(&format!("{{{{{name}[*]}}}}"), &all);
    }
    out
}

/// Рендер строки вне `__each__`: цельный плейсхолдер → JSON-значение.
fn render_string(s: &str, idx: &FieldIndex, nested: &NestedGroups) -> Value {
    let trimmed = s.trim();
    if trimmed.starts_with("{{") && trimmed.ends_with("}}") {
        let inner = &trimmed[2..trimmed.len() - 2];
        let (name, want_all) = inner
            .strip_suffix("[*]")
            .map(|n| (n, true))
            .unwrap_or((inner, false));
        if let Some(vals) = idx.get(name) {
            return if want_all {
                serde_json::to_value(vals).unwrap_or_else(|_| json!([]))
            } else {
                json!(vals.first().cloned().unwrap_or_default())
            };
        }
        if let Some(blocks) = nested.get(name) {
            return if want_all {
                serde_json::to_value(blocks).unwrap_or_else(|_| json!([]))
            } else {
                json!(
                    blocks
                        .first()
                        .and_then(|v| v.first().cloned())
                        .unwrap_or_default()
                )
            };
        }
        return json!(null);
    }
    Value::String(substitute_flat(s, idx))
}
