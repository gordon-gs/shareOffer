#!/usr/bin/env python3

import json
import argparse

def generate_config(num_servers, num_clients, start_server_port=18000, start_client_port=18009, start_conn_id=0, gw_type="tdgw"):
    config = {
        "version": "1.0",
        "description": "TCP Connection and Buffer Management",
        "share_offer_id": 49,
        "type": "socket",
        "global_settings": {
            "max_connections": 128,
            "ring_buffer_size": 8192,
            "reconnect_interval": 0,
            "connection_timeout": 0
        },
        "arp_table": [
            {
                "host_ip": "127.0.0.1",
                "is_local": True
            }
        ],
        "connections": []
    }
    
    conn_id = start_conn_id
    
    # 生成server类型的连接
    for i in range(num_servers):
        server_conn = {
            "conn_id": conn_id,
            "conn_tag": f"app-{i:02d}",
            "conn_type": "server",
            "enable": True,
            "local_ip": "127.0.0.1",
            "local_port": start_server_port + i,
            "remote_ip": "127.0.0.1",
            "remote_port": 0
        }
        config["connections"].append(server_conn)
        conn_id += 1
    
    # 生成client类型的连接
    # 根据网关类型决定conn_tag前缀
    client_prefix = "TDGW" if gw_type == "tdgw" else "TGW"
    for i in range(num_clients):
        client_conn = {
            "conn_id": conn_id,
            "conn_tag": f"{client_prefix}-{i+1:02d}",
            "conn_type": "client",
            "enable": True,
            "local_ip": "127.0.0.1",
            "local_port": 0,
            "remote_ip": "127.0.0.1",
            "remote_port": start_client_port + i
        }
        config["connections"].append(client_conn)
        conn_id += 1
    
    return config

def main():
    parser = argparse.ArgumentParser(description='Generate TCP share config JSON file')
    parser.add_argument('--servers', type=int, default=9, help='Number of server connections (default: 9)')
    parser.add_argument('--clients', type=int, default=10, help='Number of client connections (default: 10)')
    parser.add_argument('--start-server-port', type=int, default=18000, help='Start port for server connections (default: 18000)')
    parser.add_argument('--start-client-port', type=int, default=18009, help='Start port for client connections (default: 18009)')
    parser.add_argument('--start-conn-id', type=int, default=0, help='Start connection ID (default: 0)')
    parser.add_argument('--gw-type', default='tdgw', help='Gateway type (tdgw or tgw, default: tdgw)')
    
    args = parser.parse_args()
    
    config = generate_config(
        args.servers,
        args.clients,
        args.start_server_port,
        args.start_client_port,
        args.start_conn_id,
        args.gw_type
    )
    
    # 写入文件
    with open('tcp_share_config.json', 'w') as f:
        json.dump(config, f, indent=4)
    
    print(f"Generated tcp_share_config.json with {args.servers} servers and {args.clients} clients")

if __name__ == "__main__":
    main()