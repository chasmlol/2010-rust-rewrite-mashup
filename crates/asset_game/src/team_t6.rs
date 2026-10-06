use std::collections::BTreeMap;

pub fn t6_team_properties(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    if bytes.get(..8)? != b"\x80GSC\r\n\0\x06" {
        return None;
    }
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let mut at = word(24)? as usize;
    let mut uses = Vec::new();
    for _ in 0..half(50)? {
        let offset = half(at)? as usize;
        let count = *bytes.get(at + 2)?;
        let kind = *bytes.get(at + 3)?;
        let tail = bytes.get(offset..)?;
        let value = std::str::from_utf8(tail.get(..tail.iter().position(|&b| b == 0)?)?).ok()?;
        at += 4;
        for _ in 0..count {
            let address = word(at)? as usize;
            at += 4;
            if kind == 0 {
                uses.push((address, value));
            }
        }
    }
    uses.sort_unstable_by_key(|u| u.0);
    let mut values = BTreeMap::new();
    for pair in uses.windows(2) {
        let [(value_at, value), (key_at, key)] = pair else {
            unreachable!()
        };
        if key_at - value_at > 12 {
            continue;
        }
        let tail = bytes.get(key_at + 2..)?;
        if tail.starts_with(&[0x14, 0x1c, 0x1c, 0x28])
            || (key.starts_with("g_") && tail.starts_with(&[0x19, 0, 0x4e, 0x2e]))
        {
            values.insert((*key).to_owned(), (*value).to_owned());
        }
    }
    for triple in uses.windows(3) {
        let [(value_at, value), (_, key), (category_at, category)] = triple else {
            unreachable!()
        };
        if category_at - value_at <= 20
            && matches!(*category, "strings" | "music")
            && bytes
                .get(category_at + 2..)?
                .starts_with(&[0x14, 0x1c, 0x1c, 0x28])
        {
            values.insert(format!("{category}/{key}"), (*value).to_owned());
        }
    }
    for &(address, key) in &uses {
        if matches!(key, "attackers" | "defenders")
            && bytes.get(address + 2..)?.starts_with(&[0x14, 0x1c, 0x28])
        {
            values.insert(key.to_owned(), String::new());
        }
    }
    Some(values)
}
