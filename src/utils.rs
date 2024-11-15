pub fn bytes_to_number(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for &byte in bytes.iter().rev() {
        value = value * 256 + byte as u64;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_number_small() {
        let bytes = [0x01, 0x02, 0x03, 0x04];
        let number = bytes_to_number(&bytes);
        assert_eq!(number, 0x04030201); 
    }

    #[test]
    fn test_bytes_to_number_max_u64() {
        let bytes = [0xFF; 8]; 
        let number = bytes_to_number(&bytes);
        assert_eq!(number, u64::MAX);
    }

    #[test]
    #[should_panic(expected = "attempt to multiply with overflow")]
    fn test_bytes_to_number_overflow() {
        // 9 bytes would conceptually overflow a u64, but the function only returns the lower 8 bytes
        let bytes = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let number = bytes_to_number(&bytes);
        // Expect the function to truncate to the lowest 8 bytes
        assert_eq!(number, 0xFFFFFFFFFFFFFFFF);
    }

    #[test]
    fn test_bytes_to_number_empty() {
        let bytes: [u8; 0] = [];
        let number = bytes_to_number(&bytes);
        assert_eq!(number, 0);
    }
}