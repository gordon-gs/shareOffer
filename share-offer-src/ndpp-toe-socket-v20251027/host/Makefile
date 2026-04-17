build:
	make -C tcp_conn_lib/tcp_conn
	make -C tcp_conn_lib/utest
	make -C tcp_share_app
	make -C tcp_share_app_rust
clean:
	make -C tcp_conn_lib/tcp_conn clean
	make -C tcp_conn_lib/utest clean
	make -C tcp_share_app clean
	make -C tcp_share_app_rust clean
all: build
cc:clean
