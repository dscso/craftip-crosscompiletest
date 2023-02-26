pub fn read_string(slice: &[u8], size: usize) -> Option<String> {
    //assert!(2*size <= slice.len());
    let iter = (0..size).map(|i| u16::from_be_bytes([slice[2 * i], slice[2 * i + 1]]));

    std::char::decode_utf16(iter)
        .collect::<Result<String, _>>()
        .ok()
}
