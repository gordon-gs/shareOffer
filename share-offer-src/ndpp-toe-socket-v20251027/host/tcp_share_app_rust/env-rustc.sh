#
# sudo yum groupinstall "Development Tools" -y
# sudo yum install gcc gcc-c++ make cmake wget curl git -y

# 在线安装脚本
# curl https://sh.rustup.rs -sSf | sh

# 安装后, 注册(配置)环境变量
source $HOME/.cargo/env

rustc --version
cargo --version

# source /opt/rh/llvm-toolset-7/enable
export LIBCLANG_PATH=/usr/lib64/clang-private
export LD_LIBRARY_PATH=/usr/lib64/clang-private:$LD_LIBRARY_PATH
