use std::fmt::{Error, Write};

pub fn to_camelcase(s: &str, buf: &mut impl Write) -> Result<(), Error> {
    for (idx, word) in s.split('-').enumerate() {
        if idx == 0 {
            buf.write_str(word)?;
            continue;
        }

        let first_char = word.chars().next();
        if let Some(ch) = first_char {
            // Uppercase the first char and append to buf
            for ch_component in ch.to_uppercase() {
                buf.write_char(ch_component)?;
            }

            // Push the rest of the word
            buf.write_str(&word[ch.len_utf8()..])?;
        }
    }

    Ok(())
}
