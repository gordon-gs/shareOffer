### libev 源码包使用
```bash

#wget http://dist.schmorp.de/libev/Attic/libev-4.33.tar.gz
#tar zxvf libev-4.33.tar.gz -C ./

cd host/3rdparty/libev-4.33
./configure CFLAGS="-O2 -g -DEV_STANDALONE=1 -fPIC" --prefix=$(pwd)/../../tcp_conn_lib/libev
make

#
make install

```


### libuv 源码包使用
```bash

cd host/3rdparty/libuv-1.51.0

source /opt/rh/devtoolset-10/enable

sh autogen.sh
./configure CFLAGS="-O2 -g -DEV_STANDALONE=1 -fPIC" --prefix=$(pwd)/../../tcp_conn_lib/libuv
make

#
make install

```


### How To Use Json-C
```bash
cd host/3rdparty

#解压缩
tar zxvf json-c-v0.18.tgz -C ./

#
# yum install cmake3

#创建编译目录
cd json-c && mkdir -p cmake-build && cd cmake-build && ../cmake-configure  --prefix=$(pwd)/../../../tcp_conn_lib/json-c

make -j8

make install
```
