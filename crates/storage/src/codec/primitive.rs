use crate::codec::{
    Decode,
    Encode,
    Encoder,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    fuel_tx::{
        TxId,
        UtxoId,
    },
    fuel_types::BlockHeight,
};
use std::borrow::Cow;

pub struct Primitive<const SIZE: usize>;

pub struct PrimitiveEncoder<const SIZE: usize>([u8; SIZE]);

impl<const SIZE: usize> Encoder for PrimitiveEncoder<SIZE> {
    fn as_bytes(&self) -> Cow<[u8]> {
        Cow::Borrowed(&self.0[..])
    }
}

macro_rules! impl_encode {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl Encode<$ty> for Primitive<{ $size }> {
                type Encoder<'a> = PrimitiveEncoder<{ $size }>;

                fn encode(t: &$ty) -> Self::Encoder<'_> {
                    PrimitiveEncoder(t.to_be_bytes())
                }
            }
        )*
    };
}
macro_rules! impl_decode {
    ($($ty:ty, $size:expr),*) => {
        $(
            impl Decode<$ty> for Primitive<{ $size }> {
                fn decode(bytes: &[u8]) -> anyhow::Result<$ty> {
                    Ok(<$ty>::from_be_bytes(<[u8; { $size }]>::try_from(bytes)?))
                }
            }
        )*
    };
}

impl_encode! {
    u8, 1,
    u16, 2,
    u32, 4,
    BlockHeight, 4,
    DaBlockHeight, 8,
    u64, 8,
    u128, 16
}

impl_decode! {
    u8, 1,
    u16, 2,
    u32, 4,
    u64, 8,
    u128, 16
}

impl Decode<BlockHeight> for Primitive<4> {
    fn decode(bytes: &[u8]) -> anyhow::Result<BlockHeight> {
        Ok(BlockHeight::from(<[u8; 4]>::try_from(bytes)?))
    }
}

impl Decode<DaBlockHeight> for Primitive<8> {
    fn decode(bytes: &[u8]) -> anyhow::Result<DaBlockHeight> {
        Ok(DaBlockHeight::from(<[u8; 8]>::try_from(bytes)?))
    }
}

pub fn utxo_id_to_bytes(utxo_id: &UtxoId) -> [u8; TxId::LEN + 1] {
    let mut default = [0; TxId::LEN + 1];
    default[0..TxId::LEN].copy_from_slice(utxo_id.tx_id().as_ref());
    default[TxId::LEN] = utxo_id.output_index();
    default
}

impl Encode<UtxoId> for Primitive<{ TxId::LEN + 1 }> {
    type Encoder<'a> = PrimitiveEncoder<{ TxId::LEN + 1 }>;

    fn encode(t: &UtxoId) -> Self::Encoder<'_> {
        PrimitiveEncoder(utxo_id_to_bytes(t))
    }
}

impl Decode<UtxoId> for Primitive<{ TxId::LEN + 1 }> {
    fn decode(bytes: &[u8]) -> anyhow::Result<UtxoId> {
        let bytes = <[u8; TxId::LEN + 1]>::try_from(bytes)?;
        let tx_id: [u8; TxId::LEN] = bytes[0..TxId::LEN].try_into()?;
        Ok(UtxoId::new(TxId::from(tx_id), bytes[TxId::LEN]))
    }
}
