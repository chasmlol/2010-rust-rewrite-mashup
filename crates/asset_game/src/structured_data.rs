use fastfile_iw4::{Ptr, ZonePtr, ZoneStream};
use structured_data_iw4::{
    DataType, Definition, DefinitionSet, Enum, EnumArray, EnumEntry, IndexedArray, Property, Struct,
};

fn pointer(s: &ZoneStream<'_>, p: Ptr, offset: usize) -> Result<Ptr, String> {
    match s.ptr_at(p, offset).map_err(|e| e.to_string())? {
        ZonePtr::Offset(p) => Ok(s.resolve_alias(p)),
        _ => Err("unresolved structured-data pointer".into()),
    }
}
fn word(s: &ZoneStream<'_>, p: Ptr, offset: usize) -> Result<usize, String> {
    Ok(s.u32_at(p, offset).map_err(|e| e.to_string())? as usize)
}
fn string(s: &ZoneStream<'_>, p: Ptr, offset: usize) -> Result<String, String> {
    Ok(s.cstr(pointer(s, p, offset)?)
        .map_err(|e| e.to_string())?
        .to_owned())
}
fn array(
    s: &ZoneStream<'_>,
    p: Ptr,
    offset: usize,
    count: usize,
    stride: usize,
) -> Result<Option<Ptr>, String> {
    if count == 0 {
        return Ok(None);
    }
    let p = pointer(s, p, offset)?;
    let length = count
        .checked_mul(stride)
        .ok_or("structured-data array size overflow")?;
    s.slice_at(p, 0, length).map_err(|e| e.to_string())?;
    Ok(Some(p))
}
fn ty(s: &ZoneStream<'_>, p: Ptr, offset: usize) -> Result<DataType, String> {
    let category = word(s, p, offset)?;
    let value = word(s, p, offset + 4)?;
    Ok(match category {
        0 => DataType::Int,
        1 => DataType::Byte,
        2 => DataType::Bool,
        3 => DataType::String(value),
        4 => DataType::Enum(value),
        5 => DataType::Struct(value),
        6 => DataType::IndexedArray(value),
        7 => DataType::EnumArray(value),
        8 => DataType::Float,
        9 => DataType::Short,
        _ => return Err(format!("invalid structured-data category {category}")),
    })
}

pub fn capture_iw4_structured_data_def_set(
    s: &ZoneStream<'_>,
    header: Ptr,
) -> Result<(String, DefinitionSet), String> {
    let name = string(s, header, 0)?;
    let count = word(s, header, s.layout(4, 8))?;
    let mut definitions = Vec::new();
    if let Some(rows) = array(s, header, s.layout(8, 16), count, s.layout(52, 88))? {
        for i in 0..count {
            definitions.push(
                capture_definition(s, rows.at(i * s.layout(52, 88)))
                    .map_err(|error| format!("{name}: {error}"))?,
            );
        }
    }
    Ok((name, DefinitionSet { definitions }))
}
fn capture_definition(s: &ZoneStream<'_>, p: Ptr) -> Result<Definition, String> {
    let mut definition = Definition {
        version: s.i32_at(p, 0).map_err(|e| e.to_string())?,
        checksum: s.u32_at(p, 4).map_err(|e| e.to_string())?,
        root: ty(s, p, s.layout(40, 72))?,
        size: word(s, p, s.layout(48, 80))?,
        enums: Vec::new(),
        structs: Vec::new(),
        indexed_arrays: Vec::new(),
        enum_arrays: Vec::new(),
    };
    let count = word(s, p, 8)?;
    if let Some(rows) = array(s, p, s.layout(12, 16), count, s.layout(12, 16))? {
        for i in 0..count {
            let row = rows.at(i * s.layout(12, 16));
            let count = word(s, row, 0)?;
            let mut entries = Vec::new();
            if let Some(rows) = array(s, row, 8, count, s.layout(8, 16))? {
                for i in 0..count {
                    let row = rows.at(i * s.layout(8, 16));
                    entries.push(EnumEntry {
                        name: string(s, row, 0)?,
                        index: s.u16_at(row, s.layout(4, 8)).map_err(|e| e.to_string())?,
                    });
                }
            }
            definition.enums.push(Enum {
                reserved: word(s, row, 4)?,
                entries,
            });
        }
    }
    let count = word(s, p, s.layout(16, 24))?;
    if let Some(rows) = array(s, p, s.layout(20, 32), count, s.layout(16, 24))? {
        for i in 0..count {
            let row = rows.at(i * s.layout(16, 24));
            let count = word(s, row, 0)?;
            let mut properties = Vec::new();
            if let Some(rows) = array(s, row, s.layout(4, 8), count, s.layout(16, 24))? {
                for i in 0..count {
                    let row = rows.at(i * s.layout(16, 24));
                    properties.push(Property {
                        name: string(s, row, 0)?,
                        ty: ty(s, row, s.layout(4, 8))?,
                        offset: word(s, row, s.layout(12, 16))?,
                    });
                }
            }
            definition.structs.push(Struct {
                properties,
                size: word(s, row, s.layout(8, 16))?,
                bit_offset: word(s, row, s.layout(12, 20))?,
            });
        }
    }
    let count = word(s, p, s.layout(24, 40))?;
    if let Some(rows) = array(s, p, s.layout(28, 48), count, 16)? {
        for i in 0..count {
            let row = rows.at(i * 16);
            definition.indexed_arrays.push(IndexedArray {
                count: word(s, row, 0)?,
                element: ty(s, row, 4)?,
                stride: word(s, row, 12)?,
            });
        }
    }
    let count = word(s, p, s.layout(32, 56))?;
    if let Some(rows) = array(s, p, s.layout(36, 64), count, 16)? {
        for i in 0..count {
            let row = rows.at(i * 16);
            definition.enum_arrays.push(EnumArray {
                enumeration: word(s, row, 0)?,
                element: ty(s, row, 4)?,
                stride: word(s, row, 12)?,
            });
        }
    }
    definition
        .validate()
        .map_err(|e| format!("invalid structured-data definition: {e:?}"))?;
    Ok(definition)
}
