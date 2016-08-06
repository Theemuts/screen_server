/*
This huffman encoder has been copied from
https://raw.githubusercontent.com/PistonDevelopers/image/
fbe7a0c5dd90a69a3eb2bd18edd583f2156aa08f/src/jpeg/entropy.rs
*/

fn derive_codes_and_sizes(bits: &[u8]) -> (Vec<u8>, Vec<u16>) {
    let mut huffsize = vec![0u8; 256];
    let mut huffcode = vec![0u16; 256];

    let mut k = 0;
    let mut j;

    // Annex C.2
    // Figure C.1
    // Generate table of individual code lengths
    for i in 0usize..16 {
        j = 0;

        while j < bits[i] {
            huffsize[k] = i as u8 + 1;
            k += 1;
            j += 1;
        }
    }

    huffsize[k] = 0;

    // Annex C.2
    // Figure C.2
    // Generate table of huffman codes
    k = 0;
    let mut code = 0u16;
    let mut size = huffsize[0];

    while huffsize[k] != 0 {
        huffcode[k] = code;
        code += 1;
        k += 1;

        if huffsize[k] == size {
            continue
        }

        // FIXME there is something wrong with this code
        let diff = huffsize[k].wrapping_sub(size);
        code = if diff < 16  { code << diff as usize } else { 0 };

        size = size.wrapping_add(diff);
    }

    (huffsize, huffcode)
}

pub fn build_huff_lut(bits: &[u8], huffval: &[u8]) -> Vec<(u8, u16)> {
    let mut lut = vec![(17u8, 0u16); 256];
    let (huffsize, huffcode) = derive_codes_and_sizes(bits);

    for (i, &v) in huffval.iter().enumerate() {
        lut[v as usize] = (huffsize[i], huffcode[i]);
    }

    lut
}