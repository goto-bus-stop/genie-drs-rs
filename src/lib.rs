extern crate byteorder;

use std::io::{Read, Seek, SeekFrom, Error, ErrorKind};
use std::str;
use std::slice;
use byteorder::{ReadBytesExt, LittleEndian};

/// The DRS archive header.
pub struct DRSHeader {
    /// A copyright message.
    banner_msg: [u8; 40],
    /// File version. (always "1.00")
    version: [u8; 4],
    /// File password / identifier.
    password: [u8; 12],
    /// The amount of resource types (tables).
    num_resource_types: u32,
    /// Size in bytes of the metadata and tables. Resource contents start at this offset.
    directory_size: u32,
}

impl DRSHeader {
    /// Read a DRS archive header from a `Read`able handle.
    fn from<T: Read>(source: &mut T) -> Result<DRSHeader, Error> {
        let mut banner_msg = [0 as u8; 40];
        let mut version = [0 as u8; 4];
        let mut password = [0 as u8; 12];
        source.read_exact(&mut banner_msg)?;
        source.read_exact(&mut version)?;
        source.read_exact(&mut password)?;
        let num_resource_types = source.read_u32::<LittleEndian>()?;
        let directory_size = source.read_u32::<LittleEndian>()?;
        Ok(DRSHeader {
            banner_msg,
            version,
            password,
            num_resource_types,
            directory_size,
        })
    }
}

impl std::fmt::Debug for DRSHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f,
           "DRSHeader {{ banner_msg: '{}', version: '{}', password: '{}', num_resource_types: {}, directory_size: {} }}",
           str::from_utf8(&self.banner_msg).unwrap(),
           str::from_utf8(&self.version).unwrap(),
           str::from_utf8(&self.password).unwrap(),
           self.num_resource_types,
           self.directory_size
        )
    }
}

/// A table containing resource entries.
pub struct DRSTable {
    /// Type of the resource as a little-endian char array.
    resource_type: [u8; 4],
    /// Offset in the DRS archive where this table's resource entries can be found.
    offset: u32,
    /// Number of resource entries in this table.
    num_resources: u32,
    /// Resources.
    resources: Vec<DRSResource>,
}

impl DRSTable {
    /// Read a DRS table header from a `Read`able handle.
    fn from<T: Read>(source: &mut T) -> Result<DRSTable, Error> {
        let mut resource_type = [0 as u8; 4];
        source.read_exact(&mut resource_type)?;
        let offset = source.read_u32::<LittleEndian>()?;
        let num_resources = source.read_u32::<LittleEndian>()?;
        Ok(DRSTable {
            resource_type,
            offset,
            num_resources,
            resources: vec![],
        })
    }

    /// Read the table itself.
    fn read_resources<T: Read>(&mut self, source: &mut T) -> Result<(), Error> {
        for i in 0..self.num_resources {
            self.resources.push(DRSResource::from(source)?);
        }
        Ok(())
    }

    fn resources(&self) -> DRSResourceIterator {
        self.resources.iter()
    }
    fn resources_mut(&mut self) -> DRSResourceIteratorMut {
        self.resources.iter_mut()
    }

    fn get_resource(&self, id: u32) -> Result<&DRSResource, Error> {
        self.resources().find(|resource| { resource.id == id })
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Resource does not exist"))
    }
}

impl std::fmt::Debug for DRSTable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut resource_type = [0 as u8; 4];
        resource_type.clone_from_slice(&self.resource_type);
        resource_type.reverse();
        write!(f,
           "DRSTable {{ resource_type: '{}', offset: {}, num_resources: {} }}",
            str::from_utf8(&resource_type).unwrap(),
            self.offset,
            self.num_resources
        )
    }
}

/// A single resource in a DRS archive.
#[derive(Debug)]
pub struct DRSResource {
    /// The resource ID.
    id: u32,
    /// The offset into the DRS archive where the resource can be found.
    offset: u32,
    /// The size in bytes of the resource.
    size: u32,
}

impl DRSResource {
    /// Read DRS resource metadata from a `Read`able handle.
    fn from<T: Read>(source: &mut T) -> Result<DRSResource, Error> {
        let id = source.read_u32::<LittleEndian>()?;
        let offset = source.read_u32::<LittleEndian>()?;
        let size = source.read_u32::<LittleEndian>()?;
        Ok(DRSResource {
            id,
            offset,
            size,
        })
    }
}

pub type DRSTableIterator<'a> = slice::Iter<'a, DRSTable>;
pub type DRSTableIteratorMut<'a> = slice::IterMut<'a, DRSTable>;
pub type DRSResourceIterator<'a> = slice::Iter<'a, DRSResource>;
pub type DRSResourceIteratorMut<'a> = slice::IterMut<'a, DRSResource>;

/// A DRS archive.
#[derive(Debug)]
pub struct DRS<T: Read + Seek> {
    handle: T,
    header: Option<DRSHeader>,
    tables: Vec<DRSTable>,
}

impl<T: Read + Seek> DRS<T> {
    /// Create a new DRS archive reader for the given handle.
    /// The handle must be `Read`able and `Seek`able.
    pub fn new(handle: T) -> DRS<T> {
        DRS {
            handle,
            header: None,
            tables: vec![],
        }
    }

    /// Read the DRS archive header.
    fn read_header(&mut self) -> Result<(), Error> {
        self.header = Some(DRSHeader::from(&mut self.handle)?);
        Ok(())
    }

    /// Read the list of tables.
    fn read_tables(&mut self) -> Result<(), Error> {
        match self.header {
            Some(ref header) => {
                for _ in 0..header.num_resource_types {
                    let table = DRSTable::from(&mut self.handle)?;
                    self.tables.push(table);
                }
            },
            None => panic!("must read header first"),
        };
        Ok(())
    }

    /// Read the list of resources.
    fn read_dictionary(&mut self) -> Result<(), Error> {
        for table in &mut self.tables {
            table.read_resources(&mut self.handle)?;
        }
        Ok(())
    }

    /// Read the DRS archive, table, and resource metadata.
    pub fn read(&mut self) -> Result<(), Error> {
        self.read_header()?;
        self.read_tables()?;
        self.read_dictionary()?;
        Ok(())
    }

    pub fn get_table_mut(&mut self, resource_type: [u8; 4]) -> Result<&mut DRSTable, Error> {
        self.tables.iter_mut().find(|table| { table.resource_type == resource_type })
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Resource type does not exist"))
    }

    pub fn get_table(&self, resource_type: [u8; 4]) -> Result<&DRSTable, Error> {
        self.tables.iter().find(|table| { table.resource_type == resource_type })
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Resource type does not exist"))
    }

    pub fn get_resource(&self, resource_type: [u8; 4], id: u32) -> Result<&DRSResource, Error> {
        self.get_table(resource_type)?.get_resource(id)
    }

    /// Read a file from the DRS archive.
    pub fn read_resource(&mut self, resource_type: [u8; 4], id: u32) -> Result<Box<[u8]>, Error> {
        let &DRSResource { size, offset, .. } = self.get_resource(resource_type, id)?;

        self.handle.seek(SeekFrom::Start(u64::from(offset)))?;

        let mut buf = vec![0 as u8; size as usize];
        self.handle.read_exact(&mut buf)?;

        Ok(buf.into_boxed_slice())
    }

    pub fn tables(&self) -> DRSTableIterator {
        self.tables.iter()
    }
    pub fn tables_mut(&mut self) -> DRSTableIteratorMut {
        self.tables.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::str;
    use std::fs::File;

    #[test]
    fn it_works() {
        let file = File::open("test.drs").unwrap();
        let mut drs = ::DRS::new(file);
        drs.read().unwrap();
        println!("{:?}", drs);

        for table in drs.tables_mut() {
            for resource in table.resources_mut() {
                let content = drs.read_resource(table.resource_type, resource.id).unwrap();
                println!("{}: {:?}", resource.id, str::from_utf8(&content).unwrap());
            }
        }

        assert!(false);
    }
}
