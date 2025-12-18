mod include;

use clap::Parser;
use std::fs::{File};
use binrw::{BinReaderExt};
use std::io::{Seek, SeekFrom};

use include::{read_exact, string_from_bytes, Vdfs4VolumeBegins, Vdfs4SuperBlock, Vdfs4ExtendedSuperBlock, Vdfs4BaseTable, Vdfs4GenNodeDescr, Vdfs4RawBtreeHead, Vdfs4CatTreeKey};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short = 'v')]
    verbose: bool,
    /// Input file
    input_file: String,
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

    println!();
    let mut btree_n = 0;
    //
    let mut tfo = 0;
    let mut tfi = 0;
    //
    loop {
        let pos = file.stream_position().unwrap();
        //println!("Pos : {}", pos);
        let btables_block = (pos - btrees_offset)/block_size;
        if btables_block == exsb.btrees_lenght_blocks {
            println!("\nReach end of btrees (block {})", btables_block);
            break
        }

        let magic = read_exact(&mut file, 4)?;
        //println!("BTREES BLOCK {}: Magic found: {}, Offset: {}", btables_block, string_from_bytes(&magic), pos);
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
                    let key_type = 
                    if      key.record_type == 0x00 {"VDFS4_CATALOG_RECORD_DUMMY"} 
                    else if key.record_type == 0x01 {tfo+=1; "VDFS4_CATALOG_FOLDER_RECORD"}
                    else if key.record_type == 0x02 {tfi+=1; "VDFS4_CATALOG_FILE_RECORD"}
                    else if key.record_type == 0x03 {"VDFS4_CATALOG_HLINK_RECORD"}
                    else if key.record_type == 0x05 {"VDFS4_CATALOG_ILINK_RECORD"}
                    else if key.record_type == 0x10 {"VDFS4_CATALOG_UNPACK_INODE"}
                    else {"Unknown"};
                    println!("-- KEY {} - Object ID: {}, Parent ID: {}, Type: {}({}), Name: {}", i, key.object_id, key.parent_id, key.record_type, key_type, key.name_str());
                }
                let new_pos = file.stream_position().unwrap();
                let diff_pos = new_pos as i64 - pos as i64;
                //println!("-- READ {}", diff_pos);

                file.seek(SeekFrom::Current((4 * block_size as i64) /* sizeof descriptor*/ - diff_pos/* read data*/ ))?; // 4 blocks

            } else {
                file.seek(SeekFrom::Current((4 * block_size as i64) - 32 /* sizeof descriptor*/))?; // 4 blocks
            }

        } else if magic == b"\xED\xAC\xEF\x0D" {
            println!("\nBTREES BLOCK {}, Offset: {} - Premature end of btrees blocks??", btables_block, pos);
            break

        } else {
            //println!("- Found nothing! Skip 1 block");
            file.seek(SeekFrom::Current((1 * block_size as i64) - 4))?; // 1 block
        }
    }

    println!("{} {}", tfo, tfi);

    


    Ok(())
}
