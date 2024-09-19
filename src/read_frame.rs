use embedded_io_async::{Read, ReadReady};
use heapless::Vec;

use crate::{hldc, MAX_ENCODED_FRAME_SIZE};

// TODO in future versions use use ReadReady trait to remove need for huge UART buffer
// currently ReadReady is not implemented by most hall implementations

/// goal
/// resync and be fault tolerant
///  - recognise *xxx* x less then 5 as start of new package
///  - accept *---- as a new package
/// reject old frame if start of a newer read has been read
///  - any trailing character invalidates previous package
///
pub(crate) async fn read_frame<Rx>(
    rx: &mut Rx,
) -> Result<Vec<u8, MAX_ENCODED_FRAME_SIZE>, Error<Rx::Error>>
where
    Rx: Read + ReadReady,
    Rx::Error: defmt::Format,
{
    let mut frame: Vec<u8, MAX_ENCODED_FRAME_SIZE> = Vec::new();
    // MUST be larger then any existing uart buffer such that we can
    // be sure we have read everything and the current package is the
    // most up to date one. We can replace that use with read_ready which
    // tests if the uart has more bytes ready for us.
    let mut buf = [0u8; 20];
    let mut read;

    loop {
        frame.clear();

        let last_marker = loop {
            defmt::trace!("waiting to receive bytes");
            let n = rx.read(&mut buf).await.map_err(Error::Read)?;
            if n == 0 {
                return Err(Error::Eof);
            }
            read = &buf[0..n];
            defmt::trace!("read: {}", read);

            if let Some(last_marker) = read
                .iter()
                .rposition(|byte| *byte == hldc::FRAME_BOUNDARY_MARKER)
            {
                break last_marker;
            }
            defmt::debug!("did not find frame boundary in data");
        };

        defmt::trace!("last_marker: {}", last_marker);
        let Some(second_last) = read[..last_marker]
            .iter()
            .rposition(|byte| *byte == hldc::FRAME_BOUNDARY_MARKER)
        else {
            defmt::debug!("got partial frame, waiting for end to come in");
            frame.extend_from_slice(&read[last_marker..])?;
            match find_end(rx, &mut frame, &mut buf).await {
                FindEndResult::PackageFinished => return Ok(frame),
                FindEndResult::PackageOutdated => continue,
                FindEndResult::ReadError(err) => return Err(err),
            }
        };
        defmt::trace!("marker before that: {}", second_last);
        defmt::trace!("last - before last: {}", last_marker - second_last);
        defmt::trace!("hldc::MIN_FRAME_SIZE: {}", hldc::MIN_FRAME_SIZE);

        if last_marker - second_last >= hldc::MIN_FRAME_SIZE {
            if last_marker == read.len() - 1 {
                // full package inside buffer, no trailing characters
                frame.clear();
                frame.extend_from_slice(&read[second_last..=last_marker])?;
                return Ok(frame);
            }
            // got bytes past complete package, reject
            defmt::debug!("got bytes past frame end, might be new frame. Beginning again");
            continue;
        }

        // new package starts at last_marker
        defmt::debug!("got partial frame, waiting for end to come in");
        frame.clear();
        frame.extend_from_slice(&read[last_marker..])?;
        match find_end(rx, &mut frame, &mut buf).await {
            FindEndResult::PackageFinished => return Ok(frame),
            FindEndResult::PackageOutdated => continue,
            FindEndResult::ReadError(err) => return Err(err),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Error<RxError>
where
    RxError: defmt::Format + core::fmt::Debug,
{
    BufferOutOfSpace,
    Read(RxError),
    Eof,
}

impl<RxError: defmt::Format + core::fmt::Debug> From<u8> for Error<RxError> {
    fn from(_: u8) -> Self {
        Error::BufferOutOfSpace
    }
}

impl<RxError: defmt::Format + core::fmt::Debug> From<()> for Error<RxError> {
    fn from((): ()) -> Self {
        Error::BufferOutOfSpace
    }
}

enum FindEndResult<RxError>
where
    RxError: defmt::Format + core::fmt::Debug,
{
    PackageFinished,
    PackageOutdated,
    ReadError(Error<RxError>),
}

async fn find_end<const B: usize, const FRAME_CAPACITY: usize, Rx>(
    rx: &mut Rx,
    frame: &mut Vec<u8, FRAME_CAPACITY>,
    buf: &mut [u8; B],
) -> FindEndResult<Rx::Error>
where
    Rx: Read + ReadReady,
    Rx::Error: defmt::Format,
{
    let mut read;
    let boundary = loop {
        read = match rx.read(buf).await {
            Ok(0) => return FindEndResult::ReadError(Error::Eof),
            Ok(n) => &buf[..n],
            Err(e) => return FindEndResult::ReadError(Error::Read(e)),
        };

        if let Some(first_boundary) = read
            .iter()
            .position(|byte| *byte == hldc::FRAME_BOUNDARY_MARKER)
        {
            break first_boundary;
        }

        if let Err(()) = frame.extend_from_slice(read) {
            return FindEndResult::ReadError(Error::BufferOutOfSpace);
        }
    };

    let read_ready = match rx.read_ready(){
        Ok(is_ready) => is_ready,
        Err(e) => return FindEndResult::ReadError(Error::Read(e))
    };

    if boundary == read.len() - 1 && !read_ready {
        if let Err(()) = frame.extend_from_slice(read) {
            return FindEndResult::ReadError(Error::BufferOutOfSpace);
        }
        FindEndResult::PackageFinished
    } else {
        defmt::debug!("got bytes past frame end, might be new frame. Beginning again");
        FindEndResult::PackageOutdated
    }
}

/// legend: x rubish/faults, - data, * boundary marker
/// ----**----     -----
/// InFrame         EOF
#[cfg(test)]
mod test {
    use super::{read_frame, Error};
    use crate::hldc::FRAME_BOUNDARY_MARKER as FB;
    use core::convert::Infallible;
    use embedded_io_async::{ErrorType, Read};
    use futures::executor::block_on;

    struct MockRx {
        curr_read: usize,
        reads: &'static [&'static [u8]],
    }

    impl ErrorType for MockRx {
        type Error = Infallible;
    }

    impl Read for MockRx {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            let Some(to_read) = self.reads.get(self.curr_read) else {
                return Ok(0); //eof
            };

            assert!(
                to_read.len() <= buf.len(),
                "the mockrx only supports making up to the read buffer of data available each read"
            );
            buf[..to_read.len()].copy_from_slice(&to_read[..]);
            self.curr_read += 1;
            Ok(to_read.len())
        }
    }

    #[test]
    fn first_read_ends_in_2_boundaries() {
        // read 1           read 2        no read 3
        // * ------ **    --------*
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[
                &[FB, 2, 3, 4, 5, 6, 7, 8, FB, FB],
                &[255, 2, 3, 4, 5, 6, 7, 8, 9, FB],
            ],
        };
        let frame = block_on(read_frame::<20, 20, MockRx>(&mut rx)).unwrap();
        assert_eq!(&frame, &[FB, 255, 2, 3, 4, 5, 6, 7, 8, 9, FB])
    }

    #[test]
    fn eof_on_noise() {
        // read 1           read 2        no read 3
        // ------ *       xxx *-------       -*xxxxx
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[
                &[2, 3, 4, 5, 6, 7, 8, FB],
                &[20, 21, 22, FB, 1, 2, 3, 4, 5],
                &[6, FB, 25, 26, 27, 28, 29],
            ],
        };
        let err = block_on(read_frame::<20, 20, MockRx>(&mut rx)).unwrap_err();
        assert_eq!(err, Error::Eof)
    }

    #[test]
    fn eof_mid_package() {
        // read 1           read 2        no read 3
        // ----**----     -----
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[&[2, 3, 4, 5, FB, FB, 1, 2, 3], &[4, 5, 6, 7]],
        };
        let err = block_on(read_frame::<20, 20, MockRx>(&mut rx)).unwrap_err();
        assert_eq!(err, Error::Eof)
    }

    #[test]
    fn last_package_split() {
        // read 1           read 2        read 3
        // -------        ----**----         -*
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[
                &[12, 13, 14, 15, 16, 17, 18],
                &[19, 20, 21, FB, 1, 2, 3, 4, 5],
                &[6, FB],
            ],
        };
        let frame = block_on(read_frame::<20, 20, MockRx>(&mut rx)).unwrap();
        assert_eq!(&frame, &[FB, 1, 2, 3, 4, 5, 6, FB])
    }

    #[test]
    fn huge_read() {
        // read 1
        // ------------------------------**------*
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[&[
                255, 2, 3, 4, 5, 6, 7, 8, 9, 10, 255, 22, 23, 24, 25, 26, 27, 28, 29, 255, 2, 3, 4,
                5, 6, 7, 8, 9, 10, FB, 1, 2, 3, 4, 5, 6, FB,
            ]],
        };
        let frame = block_on(read_frame::<40, 8, MockRx>(&mut rx)).unwrap();
        assert_eq!(&frame, &[FB, 1, 2, 3, 4, 5, 6, FB])
    }

    #[test]
    fn end_in_many_small_reads() {
        // read 1             read 2    read 3  ... read 12   read 13
        // *---------------     -         -            -         *
        let mut rx = MockRx {
            curr_read: 0,
            reads: &[
                &[FB, 2, 3, 4, 5, 6, 7, 8, 9, 10, 255, 22, 23, 24, 25],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[5],
                &[FB],
            ],
        };
        let frame = block_on(read_frame::<80, 80, MockRx>(&mut rx)).unwrap();
        assert_eq!(
            &frame,
            &[
                FB, 2, 3, 4, 5, 6, 7, 8, 9, 10, 255, 22, 23, 24, 25, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                5, FB
            ]
        )
    }
}
