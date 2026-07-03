#![allow(dead_code, non_camel_case_types)]

use binrw::{BinRead, BinReaderExt};
use std::io::{self, Read, Cursor};

pub fn read_exact<R: Read>(reader: &mut R, size: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; size];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

pub fn string_from_bytes(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}

// 0x0 offset
#[derive(BinRead, PartialEq, Eq, Debug)]
pub struct Vdfs4VolumeBegins {
    pub signature: [u8; 4], //VDFS
    pub layout_version: [u8; 4], //2006, 2007
    pub command_line: [u8; 456], //command line arguments used to create the image
    pub creation_time: [u8; 16],
    pub creator_username: [u8; 16],
    _reserved: [u8; 12],
    pub checksum: u32,
}

// at 0x200 offset and a 2nd copy at 0x400 offset
#[derive(BinRead, PartialEq, Eq, Debug)]
pub struct Vdfs4SuperBlock {
    pub signature: [u8; 4], //VDFS
    pub layout_version: [u8; 4], //2006, 2007
    pub maximum_blocks_count: u64,
    _creation_timestamp: [u8; 12], //dontcare
    _volume_uuid: [u8; 16],        //dontcare
    pub volume_name: [u8; 16],
    _mkfs_version: [u8; 64],        //dontcare " The mkfs tool version used to generate Volume"
    _unused: [u8; 40],
    pub log_block_size: u8,     // log2 of block size in bytes
    pub log_super_page_size: u8, // Metadata bnode size and alignment
    pub log_erase_block_size: u8, //Discard request granularity (??? whatever that means )
    pub case_insensitive: u8, //  Case insensitive mode
    pub read_only: u8, // is read only image
    pub image_crc32_present: u8, //self explanatory, dontcare
    _force_full_decomp_decrypt: u8, //encryption related , dontcare
    _hash_type: u8, //see above
    _encryption_flags: u8,
    _sign_type: u8,
    _reserved: [u8; 54],
    _exsb_checksum: u32,
    _basetable_checksum: u32,
    _meta_hashtable_checksum: u32,
    pub image_inode_count: u64,
    _pad: u32,
    _sb_hash:[u8; 256], //RSA enctypted hash code of superblock
    pub checksum: u32,
}

// at 0x600 offset
#[derive(BinRead, PartialEq, Eq, Debug)]
pub struct Vdfs4ExtendedSuperBlock {
    pub files_count: u64,
    pub folders_count: u64, 
    //Extent describing the volume
    pub volume_start_block: u64,
    pub volume_lenght_blocks: u64, 
    pub mount_count: u32,
    _sync_count: u32,
    _unmount_count: u32,
    _generation: u32,
    //Debug area position
    pub debug_area_start_block: u64,
    pub debug_area_lenght_blocks: u64,
    pub meta_tbc: u32, //btrees extents total block count
    _pad: u32,
    //translation tables extents
    pub tables_start_block: u64,
    pub tables_lenght_blocks: u64, 
    //btrees extents
    pub btrees_start_block: u64,
    pub btrees_lenght_blocks: u64, 
    _un: [u8; 1520],//supposed to be 96 btrees exctensts but its always 1 so dikdidk
    _extension: [u8; 16],
    pub volume_blocks_count: u64,
    _crc: u8,
    _volume_uuid: [u8; 16],
    _reserved: [u8; 7],
    pub kbytes_written: u64,
    //meta hash table extent
    pub meta_hashtable_start_block: u64,
    pub meta_hashtable_lenght_blocks: u64,
    _reserved2: [u8; 860],
    pub checksum: u32,
}

#[derive(BinRead, Debug)]
pub struct Vdfs4RawBtreeHead {
    _magic: [u8; 4], //eHND
    _version1: u32,
    _version2: u32,
    pub root_bnode_id: u32,
    pub btree_height: u16,
    _padding: u16,
}


#[derive(BinRead, Debug)]
pub struct Vdfs4GenNodeDescr {
    _magic: [u8; 4], //4E 64 00 00 //Nd
    _version1: u32,
    _version2: u32,
    _free_space: u16,  //Free space left in bnode
    pub recs_count: u16, //Amount of records that this bnode contains
    pub node_id: u32, //Node id
    _prev_node_id: u32, //Node id of left sibling
    _next_node_id: u32, //Node id of right sibling
    pub node_type: u32, //Type of bnode node or index (value of enum vdfs4_node_type)
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum KeyRecordType {
    VDFS4_CATALOG_RECORD_DUMMY = 0,
    VDFS4_CATALOG_FOLDER_RECORD = 1,
    VDFS4_CATALOG_FILE_RECORD = 2,
    VDFS4_CATALOG_HLINK_RECORD = 3,
    /* UNUSED:								0x04 */
    VDFS4_CATALOG_ILINK_RECORD=5, 
    VDFS4_CATALOG_UNPACK_INODE= 0x10,
    UNKNOWN = 0xff
}
impl KeyRecordType {
    pub fn from(id: u8) -> Self {
        match id {
            0 => KeyRecordType::VDFS4_CATALOG_RECORD_DUMMY,
            1 => KeyRecordType::VDFS4_CATALOG_FOLDER_RECORD,
            2 => KeyRecordType::VDFS4_CATALOG_FILE_RECORD,
            3 => KeyRecordType::VDFS4_CATALOG_HLINK_RECORD,
            5 => KeyRecordType::VDFS4_CATALOG_ILINK_RECORD,
            0x10 => KeyRecordType::VDFS4_CATALOG_UNPACK_INODE,
            _ => KeyRecordType::UNKNOWN
        }
    }
}

pub enum InodeMeta {
    File(Vdfs4CatalogFileRecord),
    Folder(Vdfs4CatalogFolderRecord),
    None
}


#[derive(BinRead, Debug)]
pub struct Vdfs4BaseTable {
    //vdfs4_snapshot_descriptor
    pub magic: [u8; 4], //CoWB
    pub sync_count: u32,
    pub mount_count: u64,
    pub checksum_offset: u64,
    //
    pub last_page_index: [u64; 5],
    pub translation_table_offsets: [u64; 5],
}

//On-disk structure to catalog tree keys.
#[derive(BinRead, Debug)]
pub struct Vdfs4CatTreeKey {
    // vdfs4_generic_key
    _magic: [u8; 4], //00 00 00 00?
    pub key_len: u16,                    //Length of tree-specific key
    pub record_len: u16,                 //Full length of record containing the key
    //
    pub parent_id: u64,
    pub object_id: u64,
    pub record_type: u8,
    pub name_len: u8,
    #[br(count = name_len)] pub name: Vec<u8>,

    #[br(count = key_len - name_len as u16 - 26)] _padding: Vec<u8>, //all padded to 8 , can be calculated using this. 26 is size of all fields
}
impl Vdfs4CatTreeKey {
    pub fn name_str(&self) -> String {
        string_from_bytes(&self.name)
    }
}

//The eMMCFS stores dates in unsigned 64-bit integer seconds and unsigned 32-bit integer nanoseconds.
#[derive(BinRead, Debug)]
pub struct Vdfs4Timespec {
    pub seconds: u32,
    pub seconds_high: u32,
    pub nanoseconds: u32,
}

#[derive(BinRead, Debug)]
pub struct Vdfs4iExtent {
    pub begin: u64,     //start block
    pub lenght: u64,    //length in blocks
    pub iblock: u64,    //extent start block logical index(???)
}

static VDFS4_EXTENTS_COUNT_IN_FORK: usize = 9;

//The VDFS4 fork structure.
#[derive(BinRead, Debug)]
pub struct Vdfs4Fork {
    pub size_in_bytes: u64,
    pub total_blocks_count: u64,
    raw: [u8; 216], //VDFS4_EXTENTS_COUNT_IN_FORK * 24(size of Vdfs4iExtent)
}
impl Vdfs4Fork {    //in original there is  union
    pub fn inline_data(&self) -> &[u8] {
        &self.raw[..self.size_in_bytes as usize]
    }
    pub fn extents(&self) -> Result<Vec<Vdfs4iExtent>, binrw::Error> {
        let mut c = Cursor::new(&self.raw);
        let mut extents = Vec::with_capacity(9);
        for _ in 0..9 {
            extents.push(c.read_le()?);
        }
        Ok(extents)
    }
}

//On-disk structure to hold file and folder records.
#[derive(BinRead, Debug)]
pub struct Vdfs4CatalogFolderRecord {
    pub flags: u32,
    pub generation: u32,
    pub total_items_count: u64,
    pub links_count: u64,
    pub next_orphan_id: u64,
    pub file_mode: u16,
    _pad: u16,
    pub user_id: u32,
    pub group_id: u32,
    pub creation_time: Vdfs4Timespec,
    pub modification_time: Vdfs4Timespec,
    pub access_time: Vdfs4Timespec,
}

//On-disk structure to hold file records in catalog btree.
#[derive(BinRead, Debug)]
pub struct Vdfs4CatalogFileRecord {
    pub common: Vdfs4CatalogFolderRecord,
    pub data_fork: Vdfs4Fork,
}

//On-disk structure to hold hardlink records in catalog btree.
#[derive(BinRead, Debug)]
pub struct Vdfs4CatalogHlinkRecord {
    pub file_mode: u16,
    _pad1: u16,
    _pad2: u32,
}

//inode flags
pub const HARD_LINK: u32 = 1 << 10;
pub const VDFS4_COMPRESSED_FILE: u32 = 1 << 13;
pub const VDFS4_INLINE_DATA_FILE: u32 = 1 << 19;
pub const VDFS4_COMP_INLINE_DATA_FILE: u32 = 1 << 20;


pub const VDFS4_AES_NONCE_SIZE: usize = 8;

//com
#[derive(BinRead, Debug)]
pub struct Vdfs4CompFileDescr {
    _reserved: [u8; 7],
    pub sign_type: u8,
    pub magic: [u8; 4],
    pub extents_num: u16,
    pub layout_version: u16,
    pub unpacked_size: u64,
    _crc: u32,
    pub log_chunk_size: u32,
    _aes_nonce: [u8; VDFS4_AES_NONCE_SIZE],
}

#[derive(BinRead, Debug)]
pub struct Vdfs4CompExtent {
    pub magic: [u8; 2],
    pub flags: u16,
    pub len_bytes: u32,
    pub start: u64,
}