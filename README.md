# tempbin

Self-hosted temporary text paste, file and image host.

![Screenshot](https://user-images.githubusercontent.com/7418049/181550722-773781ad-bf92-4bce-ae09-e7432263194f.png)

## Features

- Upload images by pasting on the page
- Files get deleted after 24 hours
- JavaScript not required
- Upload from terminal: `curl --upload-file image.png http://localhost:1337`

## Set up

```bash
# Clone the repository
git clone https://github.com/hizkifw/tempbin

# Run it
cd tempbin
LISTEN='127.0.0.1:1337' cargo run
```

## Configuration

Right now, most of the configuration is baked into the binary. Edit the source
code and recompile it to update.

A convenience script `build.sh` is provided to help generate statically linked
release builds.
