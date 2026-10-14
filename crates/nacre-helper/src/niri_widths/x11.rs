//! Direct X11 property access; retain Xlib's raw hint fields and legacy lengths.
use super::{Identity, Pair, SizeHints, X11};
use crate::Result;
use std::collections::{BTreeMap, BTreeSet};
use unicode_casefold::UnicodeCaseFold;
use x11rb::{
    connection::Connection,
    errors::ReplyError,
    protocol::{
        xproto::{
            AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, GetPropertyReply,
            PropMode,
        },
        ErrorKind, Event,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};
pub fn decode_hints(format: u8, correct_type: bool, values: &[u32]) -> Option<SizeHints> {
    if format != 32 || !correct_type || values.len() < 15 {
        return None;
    }
    let full = values.len() >= 18;
    Some(SizeHints {
        flags: u64::from(values[0] & if full { 1023 } else { 255 }),
        x: values[1] as i32,
        y: values[2] as i32,
        width: values[3] as i32,
        height: values[4] as i32,
        min_width: values[5] as i32,
        min_height: values[6] as i32,
        max_width: values[7] as i32,
        max_height: values[8] as i32,
        width_inc: values[9] as i32,
        height_inc: values[10] as i32,
        min_aspect: Pair {
            x: values[11] as i32,
            y: values[12] as i32,
        },
        max_aspect: Pair {
            x: values[13] as i32,
            y: values[14] as i32,
        },
        base_width: if full { values[15] as i32 } else { 0 },
        base_height: if full { values[16] as i32 } else { 0 },
        win_gravity: if full { values[17] as i32 } else { 0 },
    })
}
pub fn encode_hints(hints: &SizeHints) -> [u32; 18] {
    // XSetWMNormalHints writes zero for every field whose flag is absent.
    let pick = |mask, value: i32| {
        if hints.flags & mask != 0 {
            value as u32
        } else {
            0
        }
    };
    [
        hints.flags as u32 & 1023,
        pick(5, hints.x),
        pick(5, hints.y),
        pick(10, hints.width),
        pick(10, hints.height),
        pick(16, hints.min_width),
        pick(16, hints.min_height),
        pick(32, hints.max_width),
        pick(32, hints.max_height),
        pick(64, hints.width_inc),
        pick(64, hints.height_inc),
        pick(128, hints.min_aspect.x),
        pick(128, hints.min_aspect.y),
        pick(128, hints.max_aspect.x),
        pick(128, hints.max_aspect.y),
        pick(256, hints.base_width),
        pick(256, hints.base_height),
        pick(512, hints.win_gravity),
    ]
}
pub struct Native {
    pub connection: RustConnection,
    root: u32,
    atoms: BTreeMap<String, u32>,
    selected: BTreeSet<u32>,
}
fn vanished(error: &ReplyError) -> bool {
    matches!(error,ReplyError::X11Error(error) if error.error_kind==ErrorKind::Window)
}
impl Native {
    pub fn connect() -> Result<Self> {
        let (connection, screen) = RustConnection::connect(None)?;
        let root = connection.setup().roots[screen].root;
        Ok(Self {
            connection,
            root,
            atoms: BTreeMap::new(),
            selected: BTreeSet::new(),
        })
    }
    fn atom(&mut self, name: &str) -> Result<u32> {
        if let Some(atom) = self.atoms.get(name) {
            return Ok(*atom);
        }
        let atom = self
            .connection
            .intern_atom(false, name.as_bytes())?
            .reply()?
            .atom;
        self.atoms.insert(name.into(), atom);
        Ok(atom)
    }
    fn property(
        &mut self,
        window: u32,
        name: &str,
        length: u32,
    ) -> Result<Option<GetPropertyReply>> {
        let atom = self.atom(name)?;
        match self
            .connection
            .get_property(false, window, atom, AtomEnum::ANY, 0, length)?
            .reply()
        {
            Ok(reply)
                if reply.type_ != u32::from(AtomEnum::NONE)
                    && matches!(reply.format, 8 | 32)
                    && (name == "WM_NORMAL_HINTS" || reply.bytes_after == 0) =>
            {
                Ok(Some(reply))
            }
            Ok(_) => Ok(None),
            Err(error) if vanished(&error) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn visit(&mut self, window: u32, depth: usize, found: &mut Vec<u32>) -> Result<()> {
        found.push(window);
        if self.selected.insert(window) {
            let result = self
                .connection
                .change_window_attributes(
                    window,
                    &ChangeWindowAttributesAux::new()
                        .event_mask(EventMask::PROPERTY_CHANGE | EventMask::SUBSTRUCTURE_NOTIFY),
                )?
                .check();
            if let Err(error) = result {
                if vanished(&error) {
                    return Ok(());
                }
                return Err(error.into());
            }
        }
        if depth >= 4 {
            return Ok(());
        }
        let reply = self.connection.query_tree(window)?.reply();
        match reply {
            Ok(reply) => {
                for child in reply.children {
                    self.visit(child, depth + 1, found)?;
                }
            }
            Err(error) if vanished(&error) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
    pub fn drain(&mut self) -> Result<bool> {
        let mut changed = false;
        while let Some(event) = self.connection.poll_for_event()? {
            match event {
                Event::CreateNotify(_) | Event::DestroyNotify(_) | Event::ReparentNotify(_) => {
                    self.selected.clear()
                }
                Event::Error(error) if error.error_kind == ErrorKind::Window => {}
                Event::Error(error) => return Err(format!("X11 observer error: {error:?}").into()),
                _ => {}
            }
            changed = true;
        }
        Ok(changed)
    }
}
impl X11 for Native {
    fn windows(&mut self) -> Result<Vec<u32>> {
        let mut found = vec![];
        self.visit(self.root, 0, &mut found)?;
        self.selected.retain(|window| found.contains(window));
        Ok(found)
    }
    fn identity(&mut self, window: u32) -> Result<Option<Identity>> {
        let classes = self.property(window, "WM_CLASS", 4096)?;
        let title = self.property(window, "_NET_WM_NAME", 4096)?;
        let title = match title {
            Some(value) if !value.value.is_empty() => Some(value),
            _ => self.property(window, "WM_NAME", 4096)?,
        };
        let (Some(classes), Some(title)) = (classes, title) else {
            return Ok(None);
        };
        if classes.format != 8
            || title.format != 8
            || classes.bytes_after != 0
            || title.bytes_after != 0
        {
            return Ok(None);
        }
        let classes = classes
            .value
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).case_fold().collect())
            .collect();
        let title = String::from_utf8_lossy(&title.value)
            .trim_end_matches('\0')
            .into();
        Ok(Some((classes, title)))
    }
    fn ordinary(&mut self, window: u32) -> Result<bool> {
        if self
            .property(window, "WM_TRANSIENT_FOR", 4096)?
            .is_some_and(|value| !value.value.is_empty() && value.bytes_after == 0)
        {
            return Ok(false);
        }
        let Some(kinds) = self
            .property(window, "_NET_WM_WINDOW_TYPE", 4096)?
            .filter(|value| !value.value.is_empty() && value.bytes_after == 0)
        else {
            return Ok(true);
        };
        let normal = self.atom("_NET_WM_WINDOW_TYPE_NORMAL")?;
        Ok(kinds
            .value32()
            .is_some_and(|mut values| values.any(|value| value == normal)))
    }
    fn hints(&mut self, window: u32) -> Result<Option<SizeHints>> {
        let Some(reply) = self.property(window, "WM_NORMAL_HINTS", 18)? else {
            return Ok(None);
        };
        let values = reply
            .value32()
            .map(|values| values.collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(decode_hints(
            reply.format,
            reply.type_ == u32::from(AtomEnum::WM_SIZE_HINTS),
            &values,
        ))
    }
    fn set_hints(&mut self, window: u32, hints: &SizeHints) -> Result<()> {
        match self
            .connection
            .change_property32(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NORMAL_HINTS,
                AtomEnum::WM_SIZE_HINTS,
                &encode_hints(hints),
            )?
            .check()
        {
            Ok(()) => {}
            Err(error) if vanished(&error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        self.connection.flush()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    #[test]
    fn original_xlib_decoder_and_width_only_setter_match() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-widths-x11-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing Xlib cases")? {
            let request = &case["request"];
            let values: Vec<u32> = serde_json::from_value(request["values"].clone())?;
            let hints = decode_hints(
                request["format"].as_u64().ok_or("Missing format")? as u8,
                request["type"] == "WM_SIZE_HINTS",
                &values,
            );
            assert_eq!(
                serde_json::to_value(&hints)?,
                case["observed"]["hints"],
                "{}",
                request["name"]
            );
            if let Some(mut hints) = hints {
                hints.min_width = 1;
                let written = encode_hints(&hints);
                assert_eq!(
                    json!(written),
                    case["observed"]["written_property"],
                    "{}",
                    request["name"]
                );
                assert_eq!(
                    serde_json::to_value(decode_hints(32, true, &written))?,
                    case["observed"]["after"]
                );
            }
        }
        Ok(())
    }
}
