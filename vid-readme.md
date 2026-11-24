# Vid

A media feed manager. Supports Youtube and podcasts.

## Build the docker image

```
docker build -t vid . && docker image save -o vid.tar vid
```

## Run the docker image on our server

```
cd /share/main/server/docker/vid_storage/ && docker rm vid && docker rmi vid && docker load -i ../vid.tar && docker run -d -v $(pwd):/work --name vid vid
```

The mounted `/work` directory should contain `yt-channels.txt` and
`yt-downloaded.txt`. Podcast feeds can be listed in `podcasts-channels.txt`
(one RSS enclosure feed URL per line), with downloaded episode GUIDs tracked in
`podcasts-downloaded.txt`. Downloads and remuxed files are stored in `/work/media`.
