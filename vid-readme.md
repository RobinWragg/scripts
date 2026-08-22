```
sudo docker build -t vid . && docker image save -o vid.tar vid
cd /share/main/server/docker/vid_storage/ && docker rm vid && docker rmi vid && docker load -i ../vid.tar && docker run -d -v $(pwd):/work --name vid vid
```