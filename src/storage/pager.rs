use crate::error::{KeraDBError, Result};
use crate::types::PageType;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Magic bytes for NoSQLite files: "NSQL"
const MAGIC_BYTES: &[u8; 4] = b"NSQL";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 64;

/// Page structure: Header (64 bytes) + Data
#[derive(Debug, Clone)]
pub struct Page {
    pub page_num: u32,
    pub page_type: PageType,
    pub checksum: u32,
    pub data: Vec<u8>,
}

impl Page {
    pub fn new(page_num: u32, page_type: PageType, data: Vec<u8>) -> Self {
        let checksum = crc32fast::hash(&data);
        Self {
            page_num,
            page_type,
            checksum,
            data,
        }
    }

    pub fn verify_checksum(&self) -> bool {
        crc32fast::hash(&self.data) == self.checksum
    }
}

/// Pager manages reading and writing pages to disk
pub struct Pager {
    file: File,
    path: PathBuf,
    page_size: usize,
    page_count: u32,
}

impl Pager {
    /// Create a new database file
    pub fn create<P: AsRef<Path>>(path: P, page_size: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if path.exists() {
            return Err(KeraDBError::InvalidFormat(
                "Database file already exists".to_string(),
            ));
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;

        // Write header
        file.write_all(MAGIC_BYTES)?;
        file.write_all(&VERSION.to_le_bytes())?;
        file.write_all(&(page_size as u32).to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?; // page count
        
        // Pad to HEADER_SIZE
        let padding = vec![0u8; HEADER_SIZE - 16];
        file.write_all(&padding)?;
        file.flush()?;

        Ok(Self {
            file,
            path,
            page_size,
            page_count: 0,
        })
    }

    /// Open an existing database file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if !path.exists() {
            return Err(KeraDBError::DatabaseNotFound(
                path.display().to_string(),
            ));
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;

        // Read and validate header
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        
        if &magic != MAGIC_BYTES {
            return Err(KeraDBError::InvalidFormat(
                "Invalid magic bytes".to_string(),
            ));
        }

        let mut version_bytes = [0u8; 4];
        file.read_exact(&mut version_bytes)?;
        let version = u32::from_le_bytes(version_bytes);
        
        if version != VERSION {
            return Err(KeraDBError::VersionMismatch {
                expected: VERSION,
                actual: version,
            });
        }

        let mut page_size_bytes = [0u8; 4];
        file.read_exact(&mut page_size_bytes)?;
        let page_size = u32::from_le_bytes(page_size_bytes) as usize;

        let mut page_count_bytes = [0u8; 4];
        file.read_exact(&mut page_count_bytes)?;
        let page_count = u32::from_le_bytes(page_count_bytes);

        Ok(Self {
            file,
            path,
            page_size,
            page_count,
        })
    }

    /// Read a page from disk
    pub fn read_page(&mut self, page_num: u32) -> Result<Page> {
        if page_num >= self.page_count {
            return Err(KeraDBError::StorageError(format!(
                "Page {} does not exist",
                page_num
            )));
        }

        let offset = HEADER_SIZE + (page_num as usize * self.page_size);
        self.file.seek(SeekFrom::Start(offset as u64))?;

        // Read page header
        let mut page_type_byte = [0u8; 1];
        self.file.read_exact(&mut page_type_byte)?;
        let page_type = PageType::try_from(page_type_byte[0])?;

        let mut checksum_bytes = [0u8; 4];
        self.file.read_exact(&mut checksum_bytes)?;
        let checksum = u32::from_le_bytes(checksum_bytes);

        // Read page data
        let data_size = self.page_size - 5; // 1 byte type + 4 bytes checksum
        let mut data = vec![0u8; data_size];
        self.file.read_exact(&mut data)?;

        let page = Page {
            page_num,
            page_type,
            checksum,
            data,
        };

        if !page.verify_checksum() {
            return Err(KeraDBError::ChecksumMismatch);
        }

        Ok(page)
    }

    /// Write a page to disk
    pub fn write_page(&mut self, page: &Page) -> Result<()> {
        let offset = HEADER_SIZE + (page.page_num as usize * self.page_size);
        self.file.seek(SeekFrom::Start(offset as u64))?;

        // Write page data (pad if necessary)
        let data_size = self.page_size - 5;
        if page.data.len() > data_size {
            return Err(KeraDBError::StorageError(
                "Page data exceeds page size".to_string(),
            ));
        }

        // Create padded data and calculate checksum on padded data
        let mut padded_data = page.data.clone();
        if padded_data.len() < data_size {
            padded_data.resize(data_size, 0);
        }
        let checksum = crc32fast::hash(&padded_data);

        // Write page header
        self.file.write_all(&[page.page_type as u8])?;
        self.file.write_all(&checksum.to_le_bytes())?;

        // Write padded data
        self.file.write_all(&padded_data)?;

        self.file.flush()?;

        // Update page count if necessary
        if page.page_num >= self.page_count {
            self.page_count = page.page_num + 1;
            self.update_header()?;
        }

        Ok(())
    }

    /// Allocate a new page
    pub fn allocate_page(&mut self, page_type: PageType) -> Result<u32> {
        let page_num = self.page_count;
        let data = vec![0u8; self.page_size - 5];
        let page = Page::new(page_num, page_type, data);
        self.write_page(&page)?;
        Ok(page_num)
    }

    /// Update the database header
    fn update_header(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(12))?;
        self.file.write_all(&self.page_count.to_le_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn sync(&mut self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");

        // Create
        let pager = Pager::create(&path, 4096).unwrap();
        drop(pager);

        // Open
        let pager = Pager::open(&path).unwrap();
        assert_eq!(pager.page_size(), 4096);
        assert_eq!(pager.page_count(), 0);
    }

    #[test]
    fn test_write_and_read_page() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ndb");

        let mut pager = Pager::create(&path, 4096).unwrap();

        let data = b"Hello, NoSQLite!".to_vec();
        let page = Page::new(0, PageType::Data, data.clone());
        
        pager.write_page(&page).unwrap();
        
        let read_page = pager.read_page(0).unwrap();
        assert_eq!(read_page.page_type, PageType::Data);
        assert!(read_page.data.starts_with(&data));
    }
}
