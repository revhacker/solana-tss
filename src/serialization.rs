use curv::arithmetic::Converter;
use curv::cryptographic_primitives::commitments::hash_commitment::HashCommitment;
use curv::cryptographic_primitives::commitments::traits::Commitment;
use curv::elliptic::curves::{DeserializationError, Point, PointFromBytesError, Scalar};
use curv::BigInt;
use multi_party_eddsa::protocols::aggsig::{self, EphemeralKey, SignFirstMsg, SignSecondMsg};
use solana_sdk::signature::Signature;
use spl_memo::solana_program::pubkey::Pubkey;
use std::convert::TryInto;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum Error {
    InputTooShort { expected: usize, found: usize },
    BadBase58(bs58::decode::Error),
    InvalidPoint(PointFromBytesError),
    InvalidScalar(DeserializationError),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooShort { expected, found } => {
                write!(f, "Input too short, expected: {}, found: {}", expected, found)
            }
            Self::BadBase58(e) => write!(f, "Invalid base58: {}", e),
            Self::InvalidPoint(e) => write!(f, "Invalid Ed25519 Point: {}", e),
            Self::InvalidScalar(e) => write!(f, "Invalid Ed25519 Scalar: {}", e),
        }
    }
}
impl std::error::Error for Error {}

impl From<PointFromBytesError> for Error {
    fn from(e: PointFromBytesError) -> Self {
        Self::InvalidPoint(e)
    }
}

impl From<DeserializationError> for Error {
    fn from(e: DeserializationError) -> Self {
        Self::InvalidScalar(e)
    }
}

pub trait FieldError<T> {
    fn with_field(self, field_name: &'static str) -> Result<T, crate::Error>;
}
impl<T: Serialize> FieldError<T> for Result<T, Error> {
    fn with_field(self, field_name: &'static str) -> Result<T, crate::Error> {
        self.map_err(|error| crate::Error::DeserializationFailed { error, field_name })
    }
}

pub trait Serialize: Sized {
    fn serialize_bs58(&self) -> String {
        let mut vec = Vec::with_capacity(self.size_hint());
        self.serialize(&mut vec);
        bs58::encode(vec).into_string()
    }
    fn serialize(&self, append_to: &mut Vec<u8>);
    fn deserialize_bs58(s: impl AsRef<[u8]>) -> Result<Self, Error> {
        let out = bs58::decode(s).into_vec().map_err(Error::BadBase58)?;
        Self::deserialize(&out)
    }
    fn deserialize(b: &[u8]) -> Result<Self, Error>;
    fn size_hint(&self) -> usize;
}

#[derive(Debug, PartialEq)]
pub struct AggMessage1 {
    pub msg: aggsig::SignFirstMsg,
    pub sender: Pubkey,
}
impl AggMessage1 {
    pub fn verify_commitment(&self, msg2: &AggMessage2) -> bool {
        HashCommitment::<sha2::Sha512>::create_commitment_with_user_defined_randomness(
            &msg2.msg.R.y_coord().expect("All ed25519 points have a y coordinate"),
            &msg2.msg.blind_factor,
        ) == self.msg.commitment
    }
}

impl Serialize for AggMessage1 {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        let c_bytes = self.msg.commitment.to_bytes_array::<64>().expect("Should fit in 64 bytes");
        append_to.extend(c_bytes);
        append_to.extend(self.sender.as_ref());
    }
    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 64 + 32 {
            return Err(Error::InputTooShort { expected: 64 + 32, found: b.len() });
        }
        let commitment = BigInt::from_bytes(&b[..64]);
        let sender = Pubkey::new(&b[64..64 + 32]);
        Ok(Self { msg: SignFirstMsg { commitment }, sender })
    }
    fn size_hint(&self) -> usize {
        64 + 32
    }
}

#[derive(Debug, PartialEq)]
pub struct AggMessage2 {
    pub msg: aggsig::SignSecondMsg,
    pub sender: Pubkey,
}

impl Serialize for AggMessage2 {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.extend(&*self.msg.R.to_bytes(true));
        let blind_bytes = self.msg.blind_factor.to_bytes_array::<64>().expect("Should fit in 64 bytes");
        append_to.extend(&blind_bytes);
        append_to.extend(self.sender.as_ref());
    }
    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 32 + 64 + 32 {
            return Err(Error::InputTooShort { expected: 32 + 64 + 32, found: b.len() });
        }
        let msg_nonce = Point::from_bytes(&b[..32])?;
        let blind_factor = BigInt::from_bytes(&b[32..32 + 64]);
        let sender = Pubkey::new(&b[32 + 64..]);
        Ok(Self { sender, msg: SignSecondMsg { R: msg_nonce, blind_factor } })
    }
    fn size_hint(&self) -> usize {
        32 + 64 + 32
    }
}

#[derive(Debug, PartialEq)]
pub struct PartialSignature(pub Signature);

impl Serialize for PartialSignature {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.extend(self.0.as_ref());
    }
    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 64 {
            return Err(Error::InputTooShort { expected: 64, found: b.len() });
        }
        Ok(PartialSignature(Signature::new(&b[..64])))
    }
    fn size_hint(&self) -> usize {
        64
    }
}

#[derive(Debug)]
pub struct SecretAggStepOne {
    pub ephemeral: aggsig::EphemeralKey,
    pub second_msg: aggsig::SignSecondMsg,
}

impl PartialEq for SecretAggStepOne {
    fn eq(&self, other: &Self) -> bool {
        self.ephemeral.r.eq(&other.ephemeral.r)
            && self.ephemeral.R.eq(&self.ephemeral.R)
            && self.second_msg.eq(&self.second_msg)
    }
}

impl Serialize for SecretAggStepOne {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.extend(&*self.ephemeral.r.to_bytes());
        append_to.extend(&*self.ephemeral.R.to_bytes(true));
        append_to.extend(&*self.second_msg.R.to_bytes(true));
        append_to.extend(&self.second_msg.blind_factor.to_bytes_array::<64>().expect("blind factor is 512 bits"));
    }
    fn deserialize(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 32 + 32 + 32 + 64 {
            return Err(Error::InputTooShort { expected: 32 + 32 + 32 + 64, found: b.len() });
        }
        let r = Scalar::from_bytes(&b[..32])?;
        let ephemeral_nonce = Point::from_bytes(&b[32..64])?;
        let second_msg_nonce = Point::from_bytes(&b[64..64 + 32])?;
        let blind_factor = BigInt::from_bytes(&b[64 + 32..]);
        Ok(Self {
            second_msg: SignSecondMsg { R: second_msg_nonce, blind_factor },
            ephemeral: EphemeralKey { R: ephemeral_nonce, r },
        })
    }
    fn size_hint(&self) -> usize {
        32 + 32 + 32 + 64
    }
}

#[derive(Debug)]
pub struct SecretAggStepTwo {
    pub ephemeral: aggsig::EphemeralKey,
    pub first_messages: Vec<AggMessage1>,
}

impl PartialEq for SecretAggStepTwo {
    fn eq(&self, other: &Self) -> bool {
        self.ephemeral.r.eq(&other.ephemeral.r)
            && self.ephemeral.R.eq(&self.ephemeral.R)
            && self.first_messages.eq(&self.first_messages)
    }
}

impl Serialize for SecretAggStepTwo {
    fn serialize(&self, append_to: &mut Vec<u8>) {
        append_to.reserve(self.size_hint());
        append_to.extend(&*self.ephemeral.r.to_bytes());
        append_to.extend(&*self.ephemeral.R.to_bytes(true));
        append_to.extend((self.first_messages.len() as u64).to_le_bytes());
        for msg in &self.first_messages {
            append_to.extend(&msg.msg.commitment.to_bytes_array::<64>().expect("The commitment is 512 bits"));
            append_to.extend(msg.sender.as_ref());
        }
        println!("{:?}", append_to);
    }
    fn deserialize(mut b: &[u8]) -> Result<Self, Error> {
        let mut expected_len = 32 + 32 + 8;
        if b.len() < 32 + 32 + 8 {
            return Err(Error::InputTooShort { expected: expected_len, found: b.len() });
        }
        let ephemeral_nonce = Scalar::from_bytes(&b[..32])?;
        let ephemeral_public_nonce = Point::from_bytes(&b[32..64])?;
        let len_first_messages = u64::from_le_bytes((&b[64..64 + 8]).try_into().expect("Exactly 8 bytes")) as usize;
        expected_len += len_first_messages * (64 + 32);
        if b.len() < expected_len {
            return Err(Error::InputTooShort { expected: expected_len, found: b.len() });
        }
        b = &b[64 + 8..];
        let first_messages: Vec<_> = (0..len_first_messages)
            .map(|i| {
                let slice = &b[i * (64 + 32)..];
                let commitment = BigInt::from_bytes(&slice[..64]);
                let sender = Pubkey::new(&slice[64..64 + 32]);
                AggMessage1 { msg: SignFirstMsg { commitment }, sender }
            })
            .collect();
        Ok(Self { first_messages, ephemeral: EphemeralKey { R: ephemeral_public_nonce, r: ephemeral_nonce } })
    }
    fn size_hint(&self) -> usize {
        32 + 32 + 8 + self.first_messages.len() * (64 + 32)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AggMessage1, AggMessage2, PartialSignature, SecretAggStepOne, SecretAggStepTwo, Serialize};
    use multi_party_eddsa::protocols::{aggsig, ExpendedKeyPair};
    use solana_sdk::signature::Signature;
    use spl_memo::solana_program::pubkey::Pubkey;
    use std::fmt::Debug;

    #[derive(PartialEq, Debug)]
    struct PanicEq<T: PartialEq + Debug>(T);

    impl<T: PartialEq + Debug> Eq for PanicEq<T> {
        fn assert_receiver_is_total_eq(&self) {}
    }

    #[test]
    fn test_agg_msg1() {
        let mut msg = [0u8; 32];
        let mut sender = [0u8; 32];
        for i in 0..u8::MAX {
            sender.fill(i);
            msg.fill(i);
            let (_, msg, _) = aggsig::create_ephemeral_key_and_commit(&ExpendedKeyPair::create(), &msg);
            let aggmsg1 = AggMessage1 { msg, sender: Pubkey::new(&sender) };
            let serialized = aggmsg1.serialize_bs58();
            let deserialized = AggMessage1::deserialize_bs58(serialized).unwrap();
            assert_eq!(PanicEq(aggmsg1), PanicEq(deserialized));
        }
    }

    #[test]
    fn test_agg_msg2() {
        let mut msg = [0u8; 32];
        let mut sender = [0u8; 32];
        for i in 0..u8::MAX {
            sender.fill(i);
            msg.fill(i);
            let (_, _, msg) = aggsig::create_ephemeral_key_and_commit(&ExpendedKeyPair::create(), &msg);
            let aggmsg2 = AggMessage2 { msg, sender: Pubkey::new(&sender) };
            let serialized = aggmsg2.serialize_bs58();
            let deserialized = AggMessage2::deserialize_bs58(serialized).unwrap();
            assert_eq!(PanicEq(aggmsg2), PanicEq(deserialized));
        }
    }

    #[test]
    fn test_agg_partial_signature() {
        let mut signature = [0u8; 64];
        for i in 0..u8::MAX {
            signature.fill(i);
            let partial_sig = PartialSignature(Signature::new(&signature));
            let serialized = partial_sig.serialize_bs58();
            let deserialized = PartialSignature::deserialize_bs58(serialized).unwrap();
            assert_eq!(PanicEq(partial_sig), PanicEq(deserialized));
        }
    }

    #[test]
    fn test_serialize_secret_agg1() {
        let mut data = [0u8; 32];
        for i in 0..u8::MAX {
            data.fill(i);
            let (ephemeral, _, second_msg) = aggsig::create_ephemeral_key_and_commit(&ExpendedKeyPair::create(), &data);
            let secret_agg1 = SecretAggStepOne { second_msg, ephemeral };
            let serialized = secret_agg1.serialize_bs58();
            let deserialized = SecretAggStepOne::deserialize_bs58(serialized).unwrap();
            assert_eq!(PanicEq(secret_agg1), PanicEq(deserialized));
        }
    }

    #[test]
    fn test_serialize_secret_agg2() {
        let mut data = [0u8; 32];
        for i in 0..50 {
            data.fill(i);
            let mut ephemeral = None;
            let msgs_len = (i as usize % 13) + 1;
            let mut first_messages = Vec::with_capacity(msgs_len);
            for _ in 0..msgs_len {
                let (eph, msg, _) = aggsig::create_ephemeral_key_and_commit(&ExpendedKeyPair::create(), &data);
                ephemeral = Some(eph);
                first_messages.push(AggMessage1 { msg, sender: Pubkey::new(&data) });
            }
            let secret_step2 = SecretAggStepTwo { ephemeral: ephemeral.unwrap(), first_messages };
            let serialized = secret_step2.serialize_bs58();
            let deserialized = SecretAggStepTwo::deserialize_bs58(serialized).unwrap();
            assert_eq!(PanicEq(secret_step2), PanicEq(deserialized));
        }
    }
}