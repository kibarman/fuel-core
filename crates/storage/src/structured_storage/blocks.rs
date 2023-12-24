use crate::{
    codec::{
        postcard::Postcard,
        raw::Raw,
    },
    column::Column,
    structure::plain::Plain,
    structured_storage::TableWithStructure,
    tables::FuelBlocks,
};

impl TableWithStructure for FuelBlocks {
    type Structure = Plain<Raw, Postcard>;

    fn column() -> Column {
        Column::FuelBlocks
    }
}

crate::basic_storage_tests!(
    FuelBlocks,
    <FuelBlocks as crate::Mappable>::Key::default(),
    <FuelBlocks as crate::Mappable>::Value::default()
);
