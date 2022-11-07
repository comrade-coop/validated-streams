kitty docker run -it --rm -p 5555:5555 -p 9944:9944 --name first  stream_node &
kitty docker run -it --rm --name second stream_node &
kitty docker run -it --rm --name third stream_node &
kitty docker run -it --rm --name forth stream_node 
