```
sudo docker build -t vid .
docker image save -o vid.tar vid
docker load -i vid.tar
docker run -d -v $(pwd):/work --name vid vid
```