use core::fmt;

pub struct Hex<'a, T> {
    chunks: &'a [T],
    limit: usize,
}

impl<'a, T> Hex<'a, T> {
    #[inline]
    pub fn new(chunks: &'a [T]) -> Self {
        Self::limited(chunks, usize::MAX)
    }

    #[inline]
    pub fn limited(chunks: &'a [T], limit: usize) -> Self {
        Self { chunks, limit }
    }
}

impl<T: core::ops::Deref<Target = [u8]>> fmt::Debug for Hex<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"")?;
        let mut remaining = self.limit;
        for chunk in self.chunks {
            if remaining == 0 {
                break;
            }
            let chunk = chunk.deref();
            let len = chunk.len().min(remaining);
            remaining -= len;
            for &b in &chunk[..len] {
                // https://doc.rust-lang.org/reference/tokens.html#byte-escapes
                if b == b'\n' {
                    write!(f, "\\n")?;
                } else if b == b'\r' {
                    write!(f, "\\r")?;
                } else if b == b'\t' {
                    write!(f, "\\t")?;
                } else if b == b'\\' || b == b'"' {
                    write!(f, "\\{}", b as char)?;
                } else if b == b'\0' {
                    write!(f, "\\0")?;
                // ASCII printable
                } else if (0x20..0x7f).contains(&b) {
                    write!(f, "{}", b as char)?;
                } else {
                    write!(f, "\\x{:02x}", b)?;
                }
            }
        }
        write!(f, "\"")?;
        Ok(())
    }
}
