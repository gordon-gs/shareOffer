
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