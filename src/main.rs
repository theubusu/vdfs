mod include;

use clap::Parser;
use std::{collections::HashMap, fs::File};
use binrw::{BinReaderExt};
use std::io::{Seek, SeekFrom};

use include::*;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short = 'v')]
    verbose: bool,
    /// Input file
    input_file: String,
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
            println!("\nReach end of btrees (block {})", btables_block);
            break
        }

        let magic = read_exact(&mut file, 4)?;
        println!("BTREES BLOCK {}: Magic found: {}, Offset: {}", btables_block, string_from_bytes(&magic), pos);
        if magic == b"fsmb" {
            println!("BTREES BLOCK {}, Offset: {} - FSMB bitmap (fmsb)", btables_block, pos);
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; //1 block

        } else if magic == b"inob" {
            println!("BTREES BLOCK {}, Offset: {} - Inode bitmap (inob)", btables_block, pos);
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; // 1 block

        } else if magic == b"eHND" { // btree header
            file.seek(SeekFrom::Current(-4))?;         
             
            let btree_head: Vdfs4RawBtreeHead = file.read_le()?;
            if verbose{println!("{:?}", btree_head)};
            let btree_type = if btree_n == 0 {"VDFS4_BTREE_CATALOG"} else if btree_n == 1 {"VDFS4_BTREE_EXTENTS"} else if btree_n == 2 {"VDFS4_BTREE_XATTRS"} else {"Unknown"};

            println!("\nBtree {} ({}) - Root bnode id: {}, Btree height: {}", 
                    btree_n, btree_type, btree_head.root_bnode_id, btree_head.btree_height);
            
            file.seek(SeekFrom::Current((4 * block_size as i64) - 20 /* sizeof descriptor*/))?; // 4 blocks
            btree_n += 1;

        } else if magic == b"Nd\x00\x00" { //NODE
            file.seek(SeekFrom::Current(-4))?;
            let node_descr: Vdfs4GenNodeDescr = file.read_le()?;
            if verbose{println!("{:?}", node_descr)};
            println!("- Bnode {} - Type: {}, Record count: {}", 
                   node_descr.node_id, node_descr.node_type, node_descr.recs_count);

            if btree_n == 1 { //catalog tree - 1 i know
                for i in 0..node_descr.recs_count {
                    let key: Vdfs4CatTreeKey = file.read_le()?;
                    if verbose{println!("{:?}", key)}
                    let key_type = KeyRecordType::from(key.record_type);
                    println!("-- KEY {} - Object ID: {}, Parent ID: {}, Type: {}({:?}), Name: {}", i, key.object_id, key.parent_id, key.record_type, key_type, key.name_str());

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
                        
                        if verbose{println!("{:?}", folder_record)};
                        println!("--- Flags: {}, Total items count: {}, Links count: {}, File mode: {}, User id: {}, Group id: {}, Creation time: {}",
                                folder_record.flags, folder_record.total_items_count, folder_record.links_count, folder_record.file_mode, folder_record.user_id, folder_record.group_id, folder_record.creation_time.seconds);
                        

                        my_inodes.insert(key.object_id, MyInode { parent_id: key.parent_id, kind: key_type.clone(), name: key.name_str(), meta: InodeMeta::Folder(folder_record)});

                    } else if key_type == KeyRecordType::VDFS4_CATALOG_FILE_RECORD {
                        let file_record: Vdfs4CatalogFileRecord = file.read_le()?;
                        
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
            println!("\nBTREES BLOCK {}, Offset: {} - Premature end of btrees blocks??", btables_block, pos);
            break

        } else {
            println!("- Found nothing! Skip 1 block");
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

    //run on root dir id 1
    run_dir(1, &cat_tree, &my_inodes, "/");


    Ok(())
}

fn run_dir(id: u64, cat_tree: &HashMap<u64, Vec<(String, u64)>>, my_inodes: &HashMap<u64, MyInode>, path: &str) {

    //process children
    if let Some(children) = cat_tree.get(&id) {
        for (child_name, child_id) in children {
            let inode = my_inodes.get(child_id).expect("inode not found");

            let new_path = if path == "/" {
                format!("/{}", child_name)
            } else {
                format!("{}/{}", path, child_name)
            };
            
            //println!("{} [{:?}]", new_path, inode.kind);
            println!("{}", new_path);

            match inode.kind {
                KeyRecordType::VDFS4_CATALOG_FOLDER_RECORD => run_dir(*child_id, cat_tree, my_inodes, &new_path),
                KeyRecordType::VDFS4_CATALOG_FILE_RECORD => read_file(inode),
                _ => println!("unexpected inode kind {:?}", inode.kind)
            }
            
        }
    }
}

fn read_file(inode: &MyInode) {
    let file_record = match &inode.meta {
        InodeMeta::File(record) => record,
        _ => panic!("no file record in inode provided to read_file")
    };
}