use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    kv_store::KeyValueStore,
    structure::plain::Plain,
    structured_storage::{
        StructuredStorage,
        TableWithStructure,
    },
    tables::{
        ContractsInfo,
        ContractsLatestUtxo,
        ContractsRawCode,
    },
    StorageRead,
};
use core::ops::Deref;
use fuel_core_types::fuel_tx::ContractId;

// # Dev-note: The value of the `ContractsRawCode` has a unique implementation of serialization
// and deserialization and uses `Raw` codec. Because the value is a contract byte code represented
// by bytes, we don't use `serde::Deserialization` and `serde::Serialization` for `Vec`,
// because we don't need to store the size of the contract. We store/load raw bytes.
impl TableWithStructure for ContractsRawCode {
    type Structure = Plain<Raw, Raw>;

    fn column() -> Column {
        Column::ContractsRawCode
    }
}

impl<S> StorageRead<ContractsRawCode> for StructuredStorage<S>
where
    S: KeyValueStore<Column = Column>,
{
    fn read(
        &self,
        key: &ContractId,
        buf: &mut [u8],
    ) -> Result<Option<usize>, Self::Error> {
        self.storage
            .read(key.as_ref(), Column::ContractsRawCode, buf)
    }

    fn read_alloc(&self, key: &ContractId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.storage
            .get(key.as_ref(), Column::ContractsRawCode)
            .map(|value| value.map(|value| value.deref().clone()))
    }
}

impl TableWithStructure for ContractsInfo {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::ContractsInfo
    }
}

impl TableWithStructure for ContractsLatestUtxo {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::ContractsLatestUtxo
    }
}

crate::basic_storage_tests!(
    ContractsRawCode,
    <ContractsRawCode as crate::Mappable>::Key::from([1u8; 32]),
    vec![32u8],
    <ContractsRawCode as crate::Mappable>::OwnedValue::from(vec![32u8])
);

crate::basic_storage_tests!(
    ContractsInfo,
    <ContractsInfo as crate::Mappable>::Key::from([1u8; 32]),
    ([2u8; 32].into(), [3u8; 32].into())
);

crate::basic_storage_tests!(
    ContractsLatestUtxo,
    <ContractsLatestUtxo as crate::Mappable>::Key::from([1u8; 32]),
    <ContractsLatestUtxo as crate::Mappable>::Value::default()
);
