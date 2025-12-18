use binrw::{BinRead};
use std::io::{self, Read};

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
    #[br(count = 4)] pub signature: Vec<u8>, //VDFS
    #[br(count = 4)] pub layout_version: Vec<u8>, //2006, 2007
    #[br(count = 456)] pub command_line: Vec<u8>, //command line arguments used to create the image
    #[br(count = 16)] pub creation_time: Vec<u8>,
    #[br(count = 16)] pub creator_username: Vec<u8>,
    #[br(count = 12)] _reserved: Vec<u8>,
    pub checksum: u32,
}

// at 0x200 offset and a 2nd copy at 0x400 offset
#[derive(BinRead, PartialEq, Eq, Debug)]
pub struct Vdfs4SuperBlock {
    #[br(count = 4)] pub signature: Vec<u8>, //VDFS
    #[br(count = 4)] pub layout_version: Vec<u8>, //2006, 2007
    pub maximum_blocks_count: u64,
    #[br(count = 12)] _creation_timestamp: Vec<u8>, //dontcare
    #[br(count = 16)] _volume_uuid: Vec<u8>,        //dontcare
    #[br(count = 16)] pub volume_name: Vec<u8>,
    #[br(count = 64)] _mkfs_version: Vec<u8>,        //dontcare " The mkfs tool version used to generate Volume"
    #[br(count = 40)] _unused: Vec<u8>,
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
    #[br(count = 54)] _reserved: Vec<u8>,
    _exsb_checksum: u32,
    _basetable_checksum: u32,
    _meta_hashtable_checksum: u32,
    pub image_inode_count: u64,
    _pad: u32,
    #[br(count = 256)] _sb_hash: Vec<u8>, //RSA enctypted hash code of superblock
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
    #[br(count = 1520)] _un: Vec<u8>,//supposed to be 96 btrees exctensts but its always 1 so dikdidk
    #[br(count = 16)] _extension: Vec<u8>,
    pub volume_blocks_count: u64,
    _crc: u8,
    #[br(count = 16)] _volume_uuid: Vec<u8>,
    #[br(count = 7)] _reserved: Vec<u8>,
    pub kbytes_written: u64,
    //meta hash table extent
    pub meta_hashtable_start_block: u64,
    pub meta_hashtable_lenght_blocks: u64,
    #[br(count = 860)] __reserved: Vec<u8>,
    pub checksum: u32,
}

#[derive(BinRead, Debug)]
pub struct Vdfs4RawBtreeHead {
    #[br(count = 4)] _magic: Vec<u8>, //eHND
    _version1: u32,
    _version2: u32,
    pub root_bnode_id: u32,
    pub btree_height: u16,
    _padding: u16,
}


#[derive(BinRead, Debug)]
pub struct Vdfs4GenNodeDescr {
    #[br(count = 4)] _magic: Vec<u8>, //4E 64 00 00 //Nd
    _version1: u32,
    _version2: u32,
    _free_space: u16,  //Free space left in bnode
    pub recs_count: u16, //Amount of records that this bnode contains
    pub node_id: u32, //Node id
    _prev_node_id: u32, //Node id of left sibling
    _next_node_id: u32, //Node id of right sibling
    pub node_type: u32, //Type of bnode node or index (value of enum vdfs4_node_type)
}

#[derive(BinRead, Debug)]
pub struct Vdfs4BaseTable {
    //vdfs4_snapshot_descriptor
    #[br(count = 4)] pub magic: Vec<u8>, //CoWB
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
    #[br(count = 4)] _magic: Vec<u8>, //00 00 00 00?
    pub key_len: u16,                    //Length of tree-specific key
    pub record_len: u16,                 //Full length of record containing the key
    //
    pub parent_id: u64,
    pub object_id: u64,
    pub record_type: u8,
    pub name_len: u8,
    #[br(count = name_len)] pub name: Vec<u8>,

    #[br(count = record_len - name_len as u16 - 26)] _rest: Vec<u8>, //unknown
}
impl Vdfs4CatTreeKey {
    pub fn name_str(&self) -> String {
        string_from_bytes(&self.name)
    }
}

//The eMMCFS stores dates in unsigned 64-bit integer seconds and unsigned 32-bit integer nanoseconds.
#[derive(BinRead, Debug)]
struct Vdfs4Timespec {
    seconds: u32,
    seconds_high: u32,
    nanoseconds: u32,
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

}