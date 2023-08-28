# Png Files
Hide files inside PNG images, or retrieve them.

This is also optimized to use as few allocations as possible in order to have save on speed. Big allocations are done only for the files you are reading/inserting, and ignores all other files stored in the image. In this way it can stay fast, and you pay only for what you use.

## ⚠️ Safety Warning ⚠️
Do not rely on this for anything that requires security! This is not encrypted and can be easily decoded by knowlegeable persons (in fact, the same program can be used). Keys are visible in the file if you use a plaintext editor as well. 

# Cli flags
| flag          | Description                                                                                                                                           |
|---------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|
| -d / --decode | Decode files from PNG (conflicts with -e, -r)                                                                                                         |
| -e / --encode | Encode files into PNG (conflicts with -d, -r)                                                                                                         |
| -r / --remove | Remove encoded files from PNG (conflicts with -e, -e)                                                                                                 |
| -i / --input  | Input PNG file                                                                                                                                        |
| -o / --output | The file path to output to in encode mode (must set). The output directory to decode files to in decode mode (optional). Does nothing in remove mode. |
| files         | A space separated list of files                                                                                                                       |

Decode mode will write out requested files from input image into current directory, or directory requested from output parameter.

Encode mode will write to new output image, leaving input image intact (will overwrite if one already exists at path).

Remove mode will overwrite input image, but with the requested encoded files removed from it.
