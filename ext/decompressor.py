import sys
import struct
import os
import zlib

VDFS4_AES_NONCE_SIZE = 8

outfile = open("decompressed.bin", "ab")

with open(sys.argv[1], "rb") as file:
    #seek to the end
    file.seek(0, os.SEEK_END)
    
    #the end offset is the file size
    file_size = file.tell()
    
    #now go to end - 40(size of compressed file descriptor
    file.seek(file_size - 40)
    
    #vdfs4_comp_file_descr
    reserved = file.read(7)
    sign_type = file.read(1)
    magic = file.read(4).decode("utf-8")
    extents_num = struct.unpack('<H', file.read(2))[0]
    layout_version = struct.unpack('<H', file.read(2))[0]
    unpacked_size = struct.unpack('<Q', file.read(8))[0]
    crc = struct.unpack('<I', file.read(4))[0]
    log_chunk_size = struct.unpack('<I', file.read(4))[0]
    aes_nonce = file.read(VDFS4_AES_NONCE_SIZE)
    #
    chunk_size = 1 << log_chunk_size
    
    #For magic
    #First letter is always C
    #Zip - zlib
    #Gzp - gzip
    #Lzo - lzo
    #Zst - zstd
    
    print(f"comp file descr - Magic: {magic}, Extents num: {extents_num}, Unpacked size: {unpacked_size}, Chunk size: {chunk_size}")
    
    if magic != "CZip":
        print("Only zlib supported")
        exit()
    
    #seek to start of extents
    file.seek(file_size - 40 - (extents_num * 16))
   
    extents = []
   
    for i in range(extents_num):
        #vdfs4_comp_extent
        magic = file.read(2)
        assert magic == b"XT"
        flags = struct.unpack('<H', file.read(2))[0]
        len_bytes = struct.unpack('<I', file.read(4))[0]
        start = struct.unpack('<Q', file.read(8))[0]
        #
        
        extents.append([flags, len_bytes, start])
          
    for i, extent in enumerate(extents):
        flags = extent[0]
        len_bytes = extent[1]
        start = extent[2]
        
        print(f"XT {i + 1} - Start: {start}, Lenght: {len_bytes}, Flags: {flags}")
        
        file.seek(start)
        data = file.read(len_bytes)
        
        if flags == 0: #compressed
            out_data = zlib.decompress(data)
        else: #uncompressed
            out_data = data
            
        outfile.write(out_data)