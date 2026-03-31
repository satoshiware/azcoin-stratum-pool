use coin_azcoin::BlockTemplate;
use std::error::Error;
use std::fmt;

pub struct AzcoinJobTemplate {
    pub job_id: String,
    pub prev_hash: Vec<u8>,
    pub version: u32,
    pub nbits: u32,
    pub ntime: u32,
    pub height: u32,
    pub coinbase_value: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AdapterError {
    MissingRequiredField(&'static str),
    HexDecodeFailure {
        field: &'static str,
        message: String,
    },
    InvalidNumericConversion {
        field: &'static str,
        value: String,
    },
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::MissingRequiredField(field) => {
                write!(f, "missing required field {}", field)
            }
            AdapterError::HexDecodeFailure { field, message } => {
                write!(f, "{} hex decode failed: {}", field, message)
            }
            AdapterError::InvalidNumericConversion { field, value } => {
                write!(f, "invalid numeric conversion for {}: {}", field, value)
            }
        }
    }
}

impl Error for AdapterError {}

pub fn from_block_template(template: &BlockTemplate) -> Result<AzcoinJobTemplate, AdapterError> {
    let prev_hash = decode_prev_hash(&template.previousblockhash)?;
    let nbits = decode_nbits(&template.bits)?;
    let ntime = u32::try_from(template.curtime).map_err(|_| AdapterError::InvalidNumericConversion {
        field: "curtime",
        value: template.curtime.to_string(),
    })?;
    let height = u32::try_from(template.height).map_err(|_| AdapterError::InvalidNumericConversion {
        field: "height",
        value: template.height.to_string(),
    })?;
    let job_id = format!("{}:{}:{}", template.height, template.previousblockhash, template.curtime);

    Ok(AzcoinJobTemplate {
        job_id,
        prev_hash,
        version: template.version,
        nbits,
        ntime,
        height,
        coinbase_value: template.coinbasevalue,
    })
}

fn decode_prev_hash(hex: &str) -> Result<Vec<u8>, AdapterError> {
    if hex.trim().is_empty() {
        return Err(AdapterError::MissingRequiredField("previousblockhash"));
    }

    let mut bytes = decode_hex(hex, "previousblockhash")?;
    if bytes.len() != 32 {
        return Err(AdapterError::HexDecodeFailure {
            field: "previousblockhash",
            message: format!("decoded length {} != 32", bytes.len()),
        });
    }

    // Preserve current AZCoin pool behavior: template_mapper reverses the RPC prevhash
    // into block-header byte order before constructing jobs.
    bytes.reverse();
    Ok(bytes)
}

fn decode_nbits(hex: &str) -> Result<u32, AdapterError> {
    if hex.trim().is_empty() {
        return Err(AdapterError::MissingRequiredField("bits"));
    }

    let bytes = decode_hex(hex, "bits")?;
    if bytes.len() != 4 {
        return Err(AdapterError::InvalidNumericConversion {
            field: "bits",
            value: hex.to_string(),
        });
    }

    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn decode_hex(hex: &str, field: &'static str) -> Result<Vec<u8>, AdapterError> {
    let hex = hex.trim();
    if hex.len() % 2 != 0 {
        return Err(AdapterError::HexDecodeFailure {
            field,
            message: "odd-length hex string".to_string(),
        });
    }

    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = decode_hex_nibble(bytes[i]).ok_or_else(|| AdapterError::HexDecodeFailure {
            field,
            message: format!("invalid hex at byte {}", i),
        })?;
        let lo = decode_hex_nibble(bytes[i + 1]).ok_or_else(|| AdapterError::HexDecodeFailure {
            field,
            message: format!("invalid hex at byte {}", i + 1),
        })?;
        out.push((hi << 4) | lo);
        i += 2;
    }

    Ok(out)
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coin_azcoin::{BlockTemplate, TransactionEntry};

    #[test]
    fn test_from_block_template_maps_fields() {
        let template = BlockTemplate {
            version: 0x20000000,
            previousblockhash: "0000000000000000000000000000000000000000000000000000000000000001"
                .to_string(),
            bits: "1d00ffff".to_string(),
            curtime: 1700000000,
            height: 100,
            transactions: vec![TransactionEntry {
                data: String::new(),
                txid: None,
                hash: None,
            }],
            coinbasevalue: 5_000_000_000,
            coinbaseaux: None,
            default_witness_commitment: None,
        };

        let job = from_block_template(&template).unwrap();

        assert_eq!(
            job.job_id,
            "100:0000000000000000000000000000000000000000000000000000000000000001:1700000000"
        );
        assert_eq!(job.version, 0x20000000);
        assert_eq!(job.nbits, 0x1d00ffff);
        assert_eq!(job.ntime, 1700000000);
        assert_eq!(job.height, 100);
        assert_eq!(job.coinbase_value, 5_000_000_000);
        assert_eq!(job.prev_hash.len(), 32);
        assert_eq!(job.prev_hash[0], 0x01);
        assert_eq!(job.prev_hash[31], 0x00);
    }
}
