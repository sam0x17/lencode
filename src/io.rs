pub trait Read {
    type Error;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

pub trait Write {
    type Error;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error>;
}

#[cfg(any(feature = "std", test))]
impl<R: std::io::Read> Read for R {
    type Error = std::io::Error;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.read(buf)
    }
}

#[cfg(any(feature = "std", test))]
impl<W: std::io::Write> Write for W {
    type Error = std::io::Error;
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.write(buf)
    }
}
