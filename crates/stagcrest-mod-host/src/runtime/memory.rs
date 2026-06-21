use wasmi::{AsContext, AsContextMut, Memory};

pub fn read_bytes<T>(
    memory: &Memory,
    ctx: impl AsContext<Data = T>,
    ptr: u32,
    len: u32,
) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    let end = ptr.checked_add(len)?;
    let data = memory.data(ctx.as_context());
    let start = ptr as usize;
    let end = end as usize;
    if end > data.len() {
        return None;
    }
    Some(data[start..end].to_vec())
}

pub fn read_utf8<T>(
    memory: &Memory,
    ctx: impl AsContext<Data = T>,
    ptr: i32,
    len: i32,
) -> Option<String> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let bytes = read_bytes(memory, ctx, ptr as u32, len as u32)?;
    String::from_utf8(bytes).ok()
}

pub fn write_bytes<T>(
    memory: &Memory,
    mut ctx: impl AsContextMut<Data = T>,
    out_ptr: i32,
    out_max: i32,
    data: &[u8],
) -> Option<i32> {
    if out_ptr < 0 || out_max <= 0 {
        return None;
    }
    let out_max = out_max as usize;
    if data.len() > out_max {
        return None;
    }
    let mem = memory.data_mut(ctx.as_context_mut());
    let start = out_ptr as usize;
    let end = start.checked_add(data.len())?;
    if end > mem.len() {
        return None;
    }
    mem[start..end].copy_from_slice(data);
    Some(data.len() as i32)
}
