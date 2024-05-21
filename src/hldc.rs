use heapless::Vec;

mod error;
pub use error::Error;

/// includes frame boundaries
pub const MIN_FRAME_SIZE: usize = 6;
const ESCAPE_MARKER: u8 = 0x7d;
pub const FRAME_BOUNDARY_MARKER: u8 = 0x7e;
/// (org, replacement)
const ESCAPED: [(u8, u8); 4] = [(0x7d, 0x5d), (0x7e, 0x5e), (0x11, 0x31), (0x13, 0x33)];

/// Produces escaped (encoded) message surrounded with `FEND`
///
/// # Errors
///
/// If the passed `MAX_ENCODED_SIZE` is too small this returns
/// `HDLCError::TooMuchData`
pub(crate) async fn encode<const MAX_ENCODED_SIZE: usize>(
    data: &[u8],
) -> Result<Vec<u8, MAX_ENCODED_SIZE>, Error> {
    // -2 for the fend start and stop bytes
    if data.len() > MAX_ENCODED_SIZE / 2 - 2 {
        return Err(Error::TooMuchData);
    }

    let mut output = Vec::new();
    output.push(FRAME_BOUNDARY_MARKER)?;
    for &byte in data {
        for (org, replacement) in ESCAPED {
            if byte == org {
                output.push(ESCAPE_MARKER)?;
                output.push(replacement)?;
                continue;
            }
        }
        output.push(byte)?;
    }
    output.push(FRAME_BOUNDARY_MARKER)?;

    Ok(output)
}

/// Produces unescaped (decoded) message without `FEND` characters.
///
/// # Errors
/// The following errors can occur while decoding:
///
/// - [`HDLCError::TooMuchData`]
/// - [`HDLCError::FendCharInData`]
/// - [`HDLCError::MissingTradeChar`]
/// - [`HDLCError::MissingFirstFend`]
/// - [`HDLCError::MissingFinalFend`]
/// - [`HDLCError::TooFewData`]
///
/// See the error type documentation for more.
pub(crate) async fn decode<const MAX_DECODED_SIZE: usize>(
    input: &[u8],
) -> Result<Vec<u8, MAX_DECODED_SIZE>, Error> {
    if input.len() < 4 {
        return Err(Error::TooFewData);
    }

    if input[0] != FRAME_BOUNDARY_MARKER {
        return Err(Error::MissingFirstFend);
    }
    if input[input.len() - 1] != FRAME_BOUNDARY_MARKER {
        return Err(Error::MissingFinalFend);
    }

    let mut output = Vec::new();
    let mut input = input[1..input.len() - 1].iter();

    while let Some(&byte) = input.next() {
        if byte == ESCAPE_MARKER {
            let Some(&escaped_byte) = input.next() else {
                return Err(Error::MissingTradeChar);
            };
            let (org, _) = ESCAPED
                .iter()
                .find(|(_, escaped)| *escaped == escaped_byte)
                .ok_or(Error::FendCharInData)?;
            output.push(*org)?;
        } else {
            output.push(byte)?;
        }

        // if output.len() % 10 == 0 {
        //     // the equivalent of a yield
        //     // we yield every 10 bytes in case other
        //     // tasks need to urgently do something
        //     delay.delay_ns(1).await;
        // }
    }

    Ok(output)
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn encode_start_measumement() {
//         let mosi_data = [0x00, 0x00, 0x02, 0x01, 0x03, 0xf9];
//         let expected = [0x7e, 0x00, 0x00, 0x02, 0x01, 0x03, 0xf9, 0x7e];
//         let encoded: Vec<u8, 20> = encode(&mosi_data, ).unwrap();
//         assert_eq!(encoded[0..encoded.len()], expected);
//     }
//
//     #[test]
//     fn encode_test() {
//         let mosi_data = [0x00, 0x01, 0x00, 0xfe];
//         let expected = [0x7e, 0x00, 0x01, 0x00, 0xfe, 0x7e];
//         let encoded: Vec<u8, 15> = encode(&mosi_data).unwrap();
//         assert_eq!(encoded[0..encoded.len()], expected);
//     }
//
//     #[test]
//     fn decode_test() {
//         let expected = [0x00, 0x01, 0x00, 0xfe];
//         let mosi_data = [0x7e, 0x00, 0x01, 0x00, 0xfe, 0x7e];
//         let encoded: Vec<u8, 10> = decode(&mosi_data).unwrap();
//         assert_eq!(encoded[0..encoded.len()], expected);
//     }
// }
