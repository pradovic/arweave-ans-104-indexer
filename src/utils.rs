pub fn bytes_to_number(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for &byte in bytes.iter().rev() {
        value = value * 256 + byte as u64;
    }
    value
}
