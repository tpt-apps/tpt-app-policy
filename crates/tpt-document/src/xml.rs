//! XML to JSON-like values, so one schema and one policy work for XML and JSON.
//!
//! The rules, which the document reference repeats for users:
//!
//! - The root element is not part of the value. Its children are the fields,
//!   so `<invoice><total>10</total></invoice>` becomes `{"total": "10"}`.
//! - An element with only text becomes that text. An empty element becomes null.
//! - An attribute `id="7"` becomes the field `@id`.
//! - Child elements become fields with their names. A name that repeats becomes
//!   a list, so `<line/><line/>` becomes `"line": [..., ...]`.
//! - Text beside child elements is kept under `#text`.
//! - All XML text is read as text. Schemas check it in text mode.

use quick_xml::escape::unescape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use serde_json::{Map, Value};

/// An XML problem, with the position it was found at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    pub message: String,
}

#[derive(Default)]
struct Frame {
    name: String,
    attributes: Map<String, Value>,
    children: Map<String, Value>,
    text: String,
}

impl Frame {
    fn into_value(self) -> Value {
        let text = self.text.trim().to_string();
        if self.attributes.is_empty() && self.children.is_empty() {
            return if text.is_empty() {
                Value::Null
            } else {
                Value::String(text)
            };
        }
        let mut map = self.attributes;
        for (key, value) in self.children {
            map.insert(key, value);
        }
        if !text.is_empty() {
            map.insert("#text".to_string(), Value::String(text));
        }
        Value::Object(map)
    }
}

/// Convert an XML document to a value. The root element's children become the
/// top-level fields.
pub fn xml_to_value(xml: &str) -> Result<Value, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Value> = None;

    loop {
        let event = reader.read_event().map_err(|e| XmlError {
            message: format!(
                "XML is not well formed at byte {}: {e}",
                reader.buffer_position()
            ),
        })?;
        match event {
            Event::Start(start) => stack.push(open(&start)?),
            Event::Empty(start) => {
                let frame = open(&start)?;
                close(&mut stack, &mut root, frame);
            }
            Event::End(_) => {
                let frame = stack.pop().ok_or_else(|| XmlError {
                    message: "XML has a closing tag with no opening tag".to_string(),
                })?;
                close(&mut stack, &mut root, frame);
            }
            Event::Text(text) => {
                if let Some(frame) = stack.last_mut() {
                    frame
                        .text
                        .push_str(&text.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::GeneralRef(entity) => {
                // `&amp;`, `&lt;`, `&#65;` and so on arrive as separate events.
                if let Some(frame) = stack.last_mut() {
                    let raw = format!("&{};", entity.as_ref());
                    let decoded = unescape(&raw).map_err(|e| XmlError {
                        message: format!("entity {raw} is not known: {e}"),
                    })?;
                    frame.text.push_str(&decoded);
                }
            }
            Event::CData(data) => {
                if let Some(frame) = stack.last_mut() {
                    frame
                        .text
                        .push_str(&data.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !stack.is_empty() {
        return Err(XmlError {
            message: format!(
                "XML element <{}> is never closed",
                stack[stack.len() - 1].name
            ),
        });
    }
    match root {
        Some(Value::Object(map)) => Ok(Value::Object(map)),
        Some(_) => Err(XmlError {
            message: "the root element has no fields. Put the document's fields inside it"
                .to_string(),
        }),
        None => Err(XmlError {
            message: "XML has no root element".to_string(),
        }),
    }
}

fn open(start: &BytesStart) -> Result<Frame, XmlError> {
    let name = start.name().as_ref().to_string();
    let mut attributes = Map::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| XmlError {
            message: format!("attribute on <{name}> is not well formed: {e}"),
        })?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| XmlError {
                message: format!("attribute @{key} on <{name}> could not be read: {e}"),
            })?
            .into_owned();
        attributes.insert(format!("@{key}"), Value::String(value));
    }
    Ok(Frame {
        name,
        attributes,
        ..Frame::default()
    })
}

/// Finish an element: add its value to its parent, or make it the root.
fn close(stack: &mut [Frame], root: &mut Option<Value>, frame: Frame) {
    let name = frame.name.clone();
    if let Some(parent) = stack.last_mut() {
        insert_child(&mut parent.children, name, frame.into_value());
    } else if root.is_none() {
        // The root element: its children are the document's fields.
        // Text directly inside the root is not a field, so it is kept as a
        // non-object and refused in xml_to_value.
        *root = Some(match frame.into_value() {
            Value::Null => Value::Object(Map::new()),
            other => other,
        });
    }
}

fn insert_child(children: &mut Map<String, Value>, name: String, value: Value) {
    match children.remove(&name) {
        None => {
            children.insert(name, value);
        }
        Some(Value::Array(mut items)) => {
            items.push(value);
            children.insert(name, Value::Array(items));
        }
        Some(existing) => {
            children.insert(name, Value::Array(vec![existing, value]));
        }
    }
}
