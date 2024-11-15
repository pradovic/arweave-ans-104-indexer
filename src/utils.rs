pub fn bytes_to_number(bytes: &[u8]) -> Result<usize, String> {
    let mut value = 0usize;

    for &byte in bytes.iter().rev() {
        value = value
            .checked_mul(256)
            .ok_or("Value exceeds usize range: multiplication overflow")?;
        value = value
            .checked_add(byte as usize)
            .ok_or("Value exceeds usize range: addition overflow")?;
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_number_within_usize_range() {
        let bytes = [0x01, 0x00, 0x00, 0x00];
        assert_eq!(bytes_to_number(&bytes).unwrap(), 1);
    }

    #[test]
    fn test_bytes_to_number_exceeds_usize_range() {
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x00];
        if std::mem::size_of::<usize>() == 4 {
            assert!(bytes_to_number(&bytes).is_err());
        } else {
            assert!(bytes_to_number(&bytes).is_ok());
        }
    }

    #[test]
    fn test_bytes_to_number_empty() {
        let bytes: [u8; 0] = [];
        assert_eq!(bytes_to_number(&bytes).unwrap(), 0);
    }

    #[test]
    fn test_bytes_to_number_exact_usize_max() {
        if std::mem::size_of::<usize>() == 4 {
            let bytes = [0xFF, 0xFF, 0xFF, 0xFF];
            assert_eq!(bytes_to_number(&bytes).unwrap(), usize::MAX);
        } else {
            let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
            assert_eq!(bytes_to_number(&bytes).unwrap(), usize::MAX);
        }
    }
}
