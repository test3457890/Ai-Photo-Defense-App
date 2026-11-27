# RAW and camera compatibility

**What works:** RAW files are opened through the JPEG preview the camera embedded in the file. The preview is located with bounds-checked parsing and decoded under the normal image limits. Edits, scans and exports then operate on that preview.

**What does not work yet:** RAW development (demosaicing, sensor white balance, highlight recovery, camera colour profiles, lens profiles, high-bit-depth processing). The mature Rust decoders (`rawloader`, `rawler`/dnglab) are LGPL-2.1 and LibRaw is LGPL-2.1/CDDL. Linking them into this MIT application needs an owner licensing decision (dynamic linking, a separate helper process, or relicensing considerations). This is recorded as **Blocked**.

Preview size is camera dependent: many cameras embed a full-resolution JPEG, some only a reduced one. Exports from RAW are therefore limited to the embedded preview's resolution and its in-camera rendering.

| Format | Identification | Preview location used | Tested with |
|---|---|---|---|
| DNG | TIFF + DNGVersion tag | JPEGInterchangeFormat / JPEG strips in IFDs and SubIFDs | Synthetic test files only |
| Canon CR2 | TIFF + "CR" signature | IFD JPEG | Not tested with camera files |
| Canon CR3 | ISO-BMFF `crx` brand | Largest decodable embedded JPEG | Synthetic test file only |
| Nikon NEF | TIFF + extension | IFD / SubIFD JPEG | Synthetic test file only |
| Sony ARW, Samsung SRW | TIFF + extension | IFD JPEG | Not tested with camera files |
| Fujifilm RAF | `FUJIFILMCCD-RAW` header | Header JPEG offset/length | Synthetic test file only |
| Olympus ORF | `IIRO`/`IIRS`/`MMOR` | IFD JPEG or scan | Not tested |
| Panasonic RW2 | `IIU` | IFD JPEG or scan | Not tested |
| Pentax PEF | TIFF + extension | IFD JPEG | Not tested |

No licensed real-camera sample set was available in this environment, so no camera model is claimed as verified. Hostile-input tests cover IFD loops, truncated files and lying offsets.
