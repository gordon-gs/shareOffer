# #!  以下通常不必添加，除非有其他情况( 例如默认编译环境被修改, 在 Centos7.9 中不是 gcc-4.8.5 )
# unset PATH
# unset LD_LIBRARY_PATH
# if [ -z "$PATH" ]; then export PATH=/usr/bin:/bin:/usr/sbin:/sbin; fi
# if [ -z "$LD_LIBRARY_PATH" ]; then export LD_LIBRARY_PATH=/usr/lib64:/usr/lib:/lib64:/lib; fi
# export PATH
# export LD_LIBRARY_PATH

source /opt/rh/devtoolset-10/enable

#! 少量模块（layer2vi, yusnic, yusur_sock）使用 clang 编译
#source /opt/rh/llvm-toolset-7/enable

which gcc
which clang
