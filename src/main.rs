mod include;

use clap::Parser;
use std::{collections::HashMap, fs::{self, File, OpenOptions}, io::{self, Cursor, Read, Write}, path::{Path, PathBuf}};
use binrw::{BinReaderExt};
use std::io::{Seek, SeekFrom};
use tar::{Builder, EntryType, Header};

use include::*;
use flate2::read::ZlibDecoder;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short = 'v')]
    verbose: bool,
    /// Input file
    input_file: String,

    out_dir: String,
}

struct MyInode {
    parent_id: u64,
    kind: KeyRecordType,
    name: String,
    meta: InodeMeta,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let verbose = args.verbose;
    let file_path = args.input_file;
    println!("Input file: {}", file_path);
    let mut file = File::open(file_path)?;
    let output_directory_path = PathBuf::from(&args.out_dir);

    // volume begins block
    let vb: Vdfs4VolumeBegins = file.read_le()?;
    if verbose{println!("{:?}", vb)};

    println!("VB: Layout version: {}", string_from_bytes(&vb.layout_version));

    //superblock 1 @ 0x200
    let sb1: Vdfs4SuperBlock = file.read_le()?;
    if verbose{println!("{:?}", sb1)};

    //superblock 2 @ 0x400
    let sb2: Vdfs4SuperBlock = file.read_le()?;
    if verbose{println!("{:?}", sb2)};

    if sb1 == sb2 {
        println!("\nTwo superblocks the same!");
    } else {
        println!("\nTwo superblocks differ!");
    }

    println!("SB: Log block size: {}", sb1.log_block_size);
    let block_size: u64 = 1 << sb1.log_block_size;
    println!("SB: Block size: {}", block_size);
    println!("SB: Max block count: {}", sb1.maximum_blocks_count);
    println!("SB: Image inode count {}", sb1.image_inode_count);

    //extended superblock @ 0x600
    let exsb: Vdfs4ExtendedSuperBlock = file.read_le()?;
    if verbose{println!("{:?}", exsb)};

    println!("\nEXSB: Files count: {}", exsb.files_count);
    println!("EXSB: Folders count: {}", exsb.folders_count);
    println!("EXSB: Volume start block: {}", exsb.volume_start_block);
    println!("EXSB: Volume lenght blocks: {}", exsb.volume_lenght_blocks);
    
    println!("\nEXSB: DEBUG AREA - Start: {} ({})", exsb.debug_area_start_block, exsb.debug_area_start_block * block_size);
    println!("EXSB: DEBUG AREA - Lenght: {} ({})", exsb.debug_area_lenght_blocks, exsb.debug_area_lenght_blocks * block_size);

    println!("\nEXSB: TABLES - Start:  {} ({})", exsb.tables_start_block, exsb.tables_start_block * block_size);
    println!("EXSB: TABLES - Lenght: {} ({})", exsb.tables_lenght_blocks, exsb.tables_lenght_blocks * block_size);

    println!("\nEXSB: BTREES - Start:  {} ({})", exsb.btrees_start_block, exsb.btrees_start_block * block_size);
    println!("EXSB: BTREES - Lenght: {} ({})", exsb.btrees_lenght_blocks, exsb.btrees_lenght_blocks * block_size);

    //after this a copy of all superblocks seems to follow. we can ignore it

    let tables_offset = exsb.tables_start_block * block_size;
    file.seek(SeekFrom::Start(tables_offset))?;
    let base_table: Vdfs4BaseTable = file.read_le()?;
    if verbose{println!("{:?}", base_table)};

    let btrees_offset = exsb.btrees_start_block * block_size;
    file.seek(SeekFrom::Start(btrees_offset))?;

    //
    let mut my_inodes: HashMap<u64, MyInode> = HashMap::new();
    let mut cat_tree: HashMap<u64, Vec<(String, u64)>> = HashMap::new();

    println!("Loading...\n");
    let mut btree_n = 0;
    loop {
        let pos = file.stream_position().unwrap();
        let btables_block = (pos - btrees_offset)/block_size;
        if btables_block == exsb.btrees_lenght_blocks {
            //println!("\nReach end of btrees (block {})", btables_block);
            break
        }

        let magic = read_exact(&mut file, 4)?;
        //println!("BTREES BLOCK {}: Magic found: {}, Offset: {}", btables_block, string_from_bytes(&magic), pos);
        if magic == b"fsmb" {
            //println!("BTREES BLOCK {}, Offset: {} - FSMB bitmap (fmsb)", btables_block, pos);
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; //1 block

        } else if magic == b"inob" {
            //println!("BTREES BLOCK {}, Offset: {} - Inode bitmap (inob)", btables_block, pos);
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; // 1 block

        } else if magic == b"eHND" { // btree header
            file.seek(SeekFrom::Current(-4))?;                
            let _btree_head: Vdfs4RawBtreeHead = file.read_le()?;
            /*
            if verbose{println!("{:?}", btree_head)};
            let btree_type = if btree_n == 0 {"VDFS4_BTREE_CATALOG"} else if btree_n == 1 {"VDFS4_BTREE_EXTENTS"} else if btree_n == 2 {"VDFS4_BTREE_XATTRS"} else {"Unknown"};

            println!("\nBtree {} ({}) - Root bnode id: {}, Btree height: {}", 
                    btree_n, btree_type, btree_head.root_bnode_id, btree_head.btree_height);
            */
            
            file.seek(SeekFrom::Current((4 * block_size as i64) - 20 /* sizeof descriptor*/))?; // 4 blocks
            btree_n += 1;

        } else if magic == b"Nd\x00\x00" { //NODE
            file.seek(SeekFrom::Current(-4))?;
            let node_descr: Vdfs4GenNodeDescr = file.read_le()?;
            //if verbose{println!("{:?}", node_descr)};
            //println!("- Bnode {} - Type: {}, Record count: {}", 
            //       node_descr.node_id, node_descr.node_type, node_descr.recs_count);

            if btree_n == 1 { //catalog tree - 1 i know
                for _i in 0..node_descr.recs_count {
                    let key: Vdfs4CatTreeKey = file.read_le()?;
                    if verbose{println!("{:?}", key)}
                    let key_type = KeyRecordType::from(key.record_type);
                    //println!("-- KEY {} - Object ID: {}, Parent ID: {}, Type: {}({:?}), Name: {}", i, key.object_id, key.parent_id, key.record_type, key_type, key.name_str());

                    if key_type == KeyRecordType::VDFS4_CATALOG_ILINK_RECORD {
                        /* cattree.c -- parent_id stored as object_id and vice versa !!!*/
                        let new_child = (key.name_str(), key.parent_id);
                        let children = cat_tree.entry(key.object_id).or_default();
    
                        //prevent duplicate entries
                        if !children.contains(&new_child) {
                            children.push(new_child);
                        }

                    } else if key_type == KeyRecordType::VDFS4_CATALOG_HLINK_RECORD {
                        //insert hlink into children (???)
                        let new_child = (key.name_str(), key.object_id);
                        let children = cat_tree.entry(key.parent_id).or_default();

                        //prevent duplicate entries
                        if !children.contains(&new_child) {
                            children.push(new_child);
                        }
                        
                    } else {
                        //ran in seperate
                    }

                    if node_descr.node_type == 1 { //special type for root inode?
                        let _ = read_exact(&mut file, 4); // it has a number only idk what for

                    } else if key_type == KeyRecordType::VDFS4_CATALOG_FOLDER_RECORD {
                        let folder_record: Vdfs4CatalogFolderRecord = file.read_le()?;
                        /* 
                        if verbose{println!("{:?}", folder_record)};
                        println!("--- Flags: {}, Total items count: {}, Links count: {}, File mode: {}, User id: {}, Group id: {}, Creation time: {}",
                                folder_record.flags, folder_record.total_items_count, folder_record.links_count, folder_record.file_mode, folder_record.user_id, folder_record.group_id, folder_record.creation_time.seconds);
                        
                        */
                        my_inodes.insert(key.object_id, MyInode { parent_id: key.parent_id, kind: key_type.clone(), name: key.name_str(), meta: InodeMeta::Folder(folder_record)});

                    } else if key_type == KeyRecordType::VDFS4_CATALOG_FILE_RECORD {
                        let file_record: Vdfs4CatalogFileRecord = file.read_le()?;
                        
                        /*
                        if verbose{println!("{:?}", file_record)};
                        println!("--- Flags: {}, Total items count: {}, Links count: {}, File mode: {}, User id: {}, Group id: {}, Creation time: {}",
                                file_record.common.flags, file_record.common.total_items_count, file_record.common.links_count, file_record.common.file_mode, file_record.common.user_id, file_record.common.group_id, file_record.common.creation_time.seconds);
                        println!("--- Size in bytes: {}, Total blocks count: {}", file_record.data_fork.size_in_bytes, file_record.data_fork.total_blocks_count);

                        if (file_record.common.flags & (1 << 19 /*VDFS4_INLINE_DATA_FILE*/)) != 0 { //INLINE FILE
                            println!("--- [INLINE FILE]");
                        } else {
                            for extent in &file_record.data_fork.extents {
                                if extent.lenght == 0 {break};
                                println!("--- [EXTENT] Begin: {} ({}), Lenght: {}, iBlock: {}", extent.begin, extent.begin * block_size, extent.lenght, extent.iblock);
                            }
                        }
                         */
                        
                        my_inodes.insert(key.object_id, MyInode { parent_id: key.parent_id, kind: key_type.clone(), name: key.name_str(), meta: InodeMeta::File(file_record)});

                    } else if key_type == KeyRecordType::VDFS4_CATALOG_HLINK_RECORD {
                        let hlink_record: Vdfs4CatalogHlinkRecord = file.read_le()?;
                        if verbose{println!("{:?}", hlink_record)};
                    }

                    //ilink record does not store any extra data.
                    
                }
                let new_pos = file.stream_position().unwrap();
                let diff_pos = new_pos as i64 - pos as i64;

                file.seek(SeekFrom::Current((4 * block_size as i64) /* sizeof descriptor*/ - diff_pos/* read data*/ ))?; // 4 blocks

            } else {
                file.seek(SeekFrom::Current((4 * block_size as i64) - 32 /* sizeof descriptor*/))?; // 4 blocks
            }

        } else if magic == b"\xED\xAC\xEF\x0D" {
            //println!("\nBTREES BLOCK {}, Offset: {} - Premature end of btrees blocks??", btables_block, pos);
            break

        } else {
            //println!("- Found nothing! Skip 1 block");
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; // 1 block
        }
    }


    /*
    println!("\n\nMY INO");
    for (id, inode) in &my_inodes {
        println!("{}. parent: {}, kind: {:?}, name: {}", id, inode.parent_id, inode.kind, inode.name);
    
        if !my_inodes.contains_key(&inode.parent_id) {
            println!("- ORPHAN NODE");
        }
    }

    println!("\n\nCTREE");
    for (id, entries) in &cat_tree {
        println!("id: {} - {:?}", id, entries);
    }
    */

    //making TAR
    let out_tarfile = File::create(&output_directory_path).unwrap();
    let mut tar = Builder::new(out_tarfile);

    //run on root dir id 1
    run_dir(1, &cat_tree, &my_inodes, "", &mut file, &mut tar)?;

    tar.finish()?;


    Ok(())
}

fn run_dir(id: u64, cat_tree: &HashMap<u64, Vec<(String, u64)>>, my_inodes: &HashMap<u64, MyInode>, path: &str, file: &mut File, tarfile: &mut Builder<File>) -> Result<(), Box<dyn std::error::Error>> {

    let inode = &my_inodes[&id];
    let folder_record = match &inode.meta {
        InodeMeta::Folder(record) => record,
        _ => panic!("no file record in inode provided to read_file")
    };
    if !path.is_empty() {
        let mut header = Header::new_gnu();
        header.set_gid(folder_record.group_id as u64);
        header.set_uid(folder_record.user_id as u64);
        header.set_mode(folder_record.file_mode as u32);

        //mtime
        let mtime_secs = ((folder_record.modification_time.seconds_high as u64) << 32)| (folder_record.modification_time.seconds as u64);
        header.set_mtime(mtime_secs);

        header.set_entry_type(EntryType::Directory);
        header.set_size(0);
        tarfile.append_data(&mut header, path, std::io::empty())?;
    }

    //process children
    if let Some(children) = cat_tree.get(&id) {
        for (child_name, child_id) in children {
            let inode = my_inodes.get(child_id).expect("inode not found");

            let new_path = if path.is_empty() {
                child_name.clone()
            } else {
                format!("{}/{}", path, child_name)
            };

            //println!("{} [{:?}]", new_path, inode.kind);
            println!("{}", new_path);

            match inode.kind {
                KeyRecordType::VDFS4_CATALOG_FOLDER_RECORD => run_dir(*child_id, cat_tree, my_inodes, &new_path, file, tarfile)?,
                KeyRecordType::VDFS4_CATALOG_FILE_RECORD => read_file(inode, &new_path, file, tarfile)?,
                _ => return Err(format!("unexpected inode kind {:?}", inode.kind).into())
            }
            
        }
    }
    Ok(())
}

fn read_file(inode: &MyInode, path: &str, file: &mut File, tarfile: &mut Builder<File>) -> Result<(), Box<dyn std::error::Error>> {
    let file_record = match &inode.meta {
        InodeMeta::File(record) => record,
        _ => panic!("no file record in inode provided to read_file")
    };
    //println!("- Flags: {}, Total items count: {}, Links count: {}, File mode: {}, User id: {}, Group id: {}, Creation time: {}",
    //        file_record.common.flags, file_record.common.total_items_count, file_record.common.links_count, file_record.common.file_mode, file_record.common.user_id, file_record.common.group_id, file_record.common.creation_time.seconds);
    //println!("- Size in bytes: {}, Total blocks count: {}", file_record.data_fork.size_in_bytes, file_record.data_fork.total_blocks_count);

    let mut data: Vec<u8> = Vec::new();
    
    if (file_record.common.flags & VDFS4_INLINE_DATA_FILE) != 0 {
        //println!("- [INLINE FILE]");
        data = file_record.data_fork.inline_data().to_vec();
    } else {
        for extent in &file_record.data_fork.extents()? {
            if extent.lenght == 0 {break};
            //println!("-- [EXTENT] Begin: {}, Lenght: {}, iBlock: {}", extent.begin, extent.lenght, extent.iblock);

            let offset = extent.begin * 4096;
            let size = extent.lenght * 4096;

            file.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; size as usize];
            file.read_exact(&mut buf)?;
            data.extend_from_slice(&buf);
        }
        data.truncate(file_record.data_fork.size_in_bytes as usize);
    }

    if (file_record.common.flags & VDFS4_COMPRESSED_FILE) != 0 {
        //println!("- [COMPRESSED FILE]");
        data = decompress_data(&data)?;
    }

    //create file
    let mut header = Header::new_gnu();

    header.set_gid(file_record.common.group_id as u64);
    header.set_uid(file_record.common.user_id as u64);
    header.set_mode(file_record.common.file_mode as u32);

    //mtime
    let mtime_secs = ((file_record.common.modification_time.seconds_high as u64) << 32)| (file_record.common.modification_time.seconds as u64);
    header.set_mtime(mtime_secs);
    
    if file_record.common.file_mode & 0o170000 == 0o120000 {    //SYMLINK
        //println!(" - [SYMLINK]");
        header.set_entry_type(EntryType::Symlink);
		header.set_size(0);
        let dest = String::from_utf8(data)?;
        tarfile.append_link(&mut header, path, &dest)?;

    } else {
        header.set_entry_type(EntryType::Regular);
        header.set_size(data.len() as u64);
        tarfile.append_data(&mut header, path, data.as_slice())?;
    }

    Ok(())
    
}

fn decompress_data(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut data_reader = Cursor::new(data);

    //seek to start of desc cmpr
    data_reader.seek(SeekFrom::End(-40))?;
    let descr: Vdfs4CompFileDescr = data_reader.read_le()?;
    //println!("comp_descr - magic={:?}, extents_num={}, layout_version={}, unpacked_size={}, log_chunk_size={}"
    //        , descr.magic, descr.extents_num, descr.layout_version, descr.unpacked_size, descr.log_chunk_size);

    //For magic
    //first letter - auth type:
    // C - none 
    // I - md5 auth
    // H - sha1 auth
    // h - sha256 auth
    let hash_len = match descr.magic[0] {
        b'C' => 0,   
        b'I' => 16 , //VDFS4_MD5_HASH_LEN
        b'H' => 20,  //VDFS4_SHA1_HASH_LEN
        b'h' => 32,  //VDFS4_SHA256_HASH_LEN
        _ => return Err("unknown/invalid hash type".into())
    };

    //rest - compression type:
    //Zip - zlib
    //Gzp - gzip
    //Lzo - lzo
    //Zst - zstd
    if &descr.magic[1..] != b"Zip" {
        return Err("only zlib supported".into());
    } 

    let sign_len = match descr.sign_type {
        0x0 => 0,
        0x1 => 128, //VDFS4_RSA1024_SIGN_LEN
        0x2 => 256, //VDFS4_RSA2048_SIGN_LEN
        _ => return Err("unknown/invalid sign_type".into())
    };

    //seek to start of extents
    data_reader.seek(SeekFrom::End(-40 -sign_len -((descr.extents_num as i64 +1) * hash_len) /* each extent has hash +1 for descriptor */ -(descr.extents_num as i64 * 16)))?;
    let mut extents: Vec<Vdfs4CompExtent> = Vec::new();
    for i in 0..descr.extents_num {
        let extent: Vdfs4CompExtent = data_reader.read_le()?;
        //println!("extent {} - magic={:?}, flags={}, len_bytes={}, start={}", i+1, extent.magic, extent.flags, extent.len_bytes, extent.start);
        if &extent.magic != b"XT" {
            return Err("invalid exten magic".into());
        }
        extents.push(extent);
    }

    let mut out_data: Vec<u8> = Vec::new();
    for extent in extents {
        data_reader.seek(SeekFrom::Start(extent.start))?;
        let mut buf = vec![0u8; extent.len_bytes as usize];
        data_reader.read_exact(&mut buf)?;

        if extent.flags == 0 {  //compressed
            buf = decompress_zlib(&buf)?;
        }
        out_data.extend_from_slice(&buf);
    }

    Ok(out_data)
}

pub fn decompress_zlib(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;

    Ok(decompressed)
}