#!/usr/bin/env python3

import json
import argparse

def generate_tgw_json(num_sessions, target_comp_id_start, platform_id, socket_connect_port, socket_connect_host, password):
    """
    生成指定数量session的tgw.json文件
    
    Args:
        num_sessions (int): 要生成的session数量
        target_comp_id_start (int): target_comp_id起始数字
        platform_id (int): 平台ID，默认为1
        socket_connect_port (int): socket连接端口起始数字
        socket_connect_host (str): socket连接主机地址，默认为127.0.0.1
        password (str): 密码，默认为abc
    """
    # 基础配置
    config = {
        "reconnect_interval": 3,
        "heart_bt_int": 3,
        "default_appl_ver_id": "1.11",
        "session": []
    }
    
    # 生成session列表
    for i in range(num_sessions):
        session = {
            "sender_comp_id": "share_offer_tgw",  # app-00, app-01, ... app-99
            "target_comp_id": f"TGW-{target_comp_id_start + i:02d}",
            "platform_id": platform_id,
            "pbus": [ "398294", "077100" ],
            "socket_connect_port": socket_connect_port + i,
            "socket_connect_host": socket_connect_host,
            "password": password
        }
        config["session"].append(session)
    
    return config

def main():
    parser = argparse.ArgumentParser(description='生成tgw.json文件')
    parser.add_argument('--num-sessions', type=int, default=9, 
                       help='要生成的session数量 (默认: 9)')
    parser.add_argument('--target-comp-id-start', type=int, default=1000,
                       help='target_comp_id起始数字 (默认: 1000)')
    parser.add_argument('--platform-id', type=int, default=1,
                       help='平台ID (默认: 1)')
    parser.add_argument('--socket-connect-port', type=int, default=10000,
                       help='socket连接端口起始数字 (默认: 10000)')
    parser.add_argument('--socket-connect-host', default='127.0.0.1',
                       help='socket连接主机地址 (默认: 127.0.0.1)')
    parser.add_argument('--password', default='abc',
                       help='密码 (默认: abc)')
    parser.add_argument('--output', default='tgw.json',
                       help='输出文件名 (默认: tgw.json)')
    
    args = parser.parse_args()
    
    # 生成配置
    config = generate_tgw_json(args.num_sessions, args.target_comp_id_start, args.platform_id, 
                              args.socket_connect_port, args.socket_connect_host, args.password)
    
    # 写入文件
    with open(args.output, 'w', encoding='utf-8') as f:
        json.dump(config, f, indent=4, ensure_ascii=False)
    
    print(f"已生成 {args.num_sessions} 个session的tgw.json文件到 {args.output}")

if __name__ == "__main__":
    main()