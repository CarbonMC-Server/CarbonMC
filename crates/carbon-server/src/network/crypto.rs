use openssl::{
    encrypt::Decrypter,
    pkey::{PKey, Private},
    rsa::{Padding, Rsa},
    symm::{Cipher, Crypter, Mode},
};
use std::io;
fn error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid encrypted login")
}

pub(super) struct LoginKey {
    key: PKey<Private>,
    pub(super) public_der: Vec<u8>,
}
impl LoginKey {
    pub(super) fn new() -> io::Result<Self> {
        // EVP PKCS#1 v1.5 decryption relies on default-provider implicit
        // rejection introduced in OpenSSL 3.2. Release builds vendor OpenSSL.
        if openssl::version::number() < 0x30200000 {
            return Err(error());
        }
        let key = PKey::from_rsa(Rsa::generate(2048).map_err(|_| error())?).map_err(|_| error())?;
        let public_der = key.public_key_to_der().map_err(|_| error())?;
        Ok(Self { key, public_der })
    }
    fn decrypt(&self, bytes: &[u8]) -> io::Result<zeroize::Zeroizing<Vec<u8>>> {
        if bytes.len() != self.key.size() {
            return Err(error());
        }
        let mut decrypt = Decrypter::new(&self.key).map_err(|_| error())?;
        decrypt
            .set_rsa_padding(Padding::PKCS1)
            .map_err(|_| error())?;
        let mut output = zeroize::Zeroizing::new(vec![0; self.key.size()]);
        let length = decrypt.decrypt(bytes, &mut output).map_err(|_| error())?;
        output.truncate(length);
        Ok(output)
    }
    pub(super) fn accept(
        &self,
        secret: &[u8],
        challenge: &[u8],
        expected: &[u8; 16],
    ) -> io::Result<zeroize::Zeroizing<[u8; 16]>> {
        let secret = self.decrypt(secret);
        let challenge = self.decrypt(challenge);
        let (secret, challenge) = (secret?, challenge?);
        if secret.len() != 16 || challenge.len() != 16 || !openssl::memcmp::eq(&challenge, expected)
        {
            return Err(error());
        }
        let secret: [u8; 16] = secret.as_slice().try_into().map_err(|_| error())?;
        Ok(zeroize::Zeroizing::new(secret))
    }
}

pub(super) struct StreamCipher {
    encrypt: Crypter,
    decrypt: Crypter,
}
impl StreamCipher {
    pub(super) fn new(secret: &[u8; 16]) -> io::Result<Self> {
        let cipher = Cipher::aes_128_cfb8();
        let encrypt =
            Crypter::new(cipher, Mode::Encrypt, secret, Some(secret)).map_err(|_| error())?;
        let decrypt =
            Crypter::new(cipher, Mode::Decrypt, secret, Some(secret)).map_err(|_| error())?;
        Ok(Self { encrypt, decrypt })
    }
    pub(super) fn encrypt(&mut self, bytes: &[u8]) -> io::Result<Vec<u8>> {
        let mut output = vec![0; bytes.len() + 16];
        let count = self
            .encrypt
            .update(bytes, &mut output)
            .map_err(|_| error())?;
        if count != bytes.len() {
            return Err(error());
        }
        output.truncate(count);
        Ok(output)
    }
    pub(super) fn decrypt(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        let mut output = vec![0; bytes.len() + 16];
        let count = self
            .decrypt
            .update(bytes, &mut output)
            .map_err(|_| error())?;
        if count != bytes.len() {
            return Err(error());
        }
        bytes.copy_from_slice(&output[..count]);
        Ok(())
    }
}

pub(super) fn signed_hex(mut digest: [u8; 20]) -> String {
    let negative = digest[0] & 0x80 != 0;
    if negative {
        let mut carry = 1u16;
        for byte in digest.iter_mut().rev() {
            let value = u16::from(!*byte) + carry;
            *byte = value as u8;
            carry = value >> 8;
        }
    }
    let text: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let text = text.trim_start_matches('0');
    if text.is_empty() {
        "0".into()
    } else if negative {
        format!("-{text}")
    } else {
        text.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::encrypt::Encrypter;
    #[test]
    fn matches_independent_java_cipher_vector() {
        let key: [u8; 16] = std::array::from_fn(|i| i as u8);
        let plain: Vec<u8> = (0..32).collect();
        let mut cipher = StreamCipher::new(&key).unwrap();
        let output = cipher.encrypt(&plain).unwrap();
        let hex: String = output.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "0a22f796e1b93e9032cff804838adfc3a5e4b3ffdd47108575533e672ef5d8ef"
        );
    }
    #[test]
    fn encryption_is_continuous_across_arbitrary_read_boundaries() {
        let secret = [42; 16];
        let plain = vec![7; 4096];
        let mut send = StreamCipher::new(&secret).unwrap();
        let mut wire = send.encrypt(&plain[..123]).unwrap();
        wire.extend(send.encrypt(&plain[123..]).unwrap());
        assert_ne!(wire, plain);
        let mut recv = StreamCipher::new(&secret).unwrap();
        for chunk in wire.chunks_mut(17) {
            recv.decrypt(chunk).unwrap();
        }
        assert_eq!(wire, plain);
    }
    #[test]
    fn challenge_secret_and_ciphertext_lengths_are_verified() {
        let key = LoginKey::new().unwrap();
        let public = PKey::public_key_from_der(&key.public_der).unwrap();
        let encrypt = |bytes: &[u8]| {
            let mut e = Encrypter::new(&public).unwrap();
            e.set_rsa_padding(Padding::PKCS1).unwrap();
            let mut result = vec![0; public.size()];
            let count = e.encrypt(bytes, &mut result).unwrap();
            result.truncate(count);
            result
        };
        let secret = encrypt(&[9; 16]);
        let challenge = encrypt(&[3; 16]);
        assert_eq!(*key.accept(&secret, &challenge, &[3; 16]).unwrap(), [9; 16]);
        assert!(key.accept(&secret, &challenge, &[4; 16]).is_err());
        assert!(key
            .accept(&encrypt(&[9; 15]), &challenge, &[3; 16])
            .is_err());
        assert!(key.accept(&secret[..255], &challenge, &[3; 16]).is_err());
        assert!(key.accept(&[0; 256], &challenge, &[3; 16]).is_err());
    }
    #[test]
    fn signed_hash_format_handles_negative_and_zero_values() {
        assert_eq!(
            signed_hex(openssl::sha::sha1(b"Notch")),
            "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"
        );
        assert_eq!(
            signed_hex(openssl::sha::sha1(b"jeb_")),
            "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"
        );
        assert_eq!(signed_hex([0; 20]), "0");
        assert_eq!(signed_hex([255; 20]), "-1");
    }
}
