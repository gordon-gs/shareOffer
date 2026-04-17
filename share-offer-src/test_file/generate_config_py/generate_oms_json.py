#!/usr/bin/env python3

import json
import argparse

def generate_oms_json(num_sessions, gw_type):
    """
    生成指定数量session的oms.json文件
    
    Args:
        num_sessions (int): 要生成的session数量
        gw_type (str): 网关类型，默认为TDGW
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
            "server_id": f"app-{i:02d}",  # app-00, app-01, ... app-99
            "socket_connect_port": 0,
            "socket_connect_host": "127.0.0.1",
            "gw_type": gw_type,
            "platform_id": 0
        }
        config["session"].append(session)
    
    return config

def main():
    parser = argparse.ArgumentParser(description='生成oms.json文件')
    parser.add_argument('--num-sessions', type=int, default=9, 
                       help='要生成的session数量 (默认: 9)')
    parser.add_argument('--gw-type', default='TDGW',
                       help='网关类型 (默认: TDGW)')
    parser.add_argument('--output', default='oms.json',
                       help='输出文件名 (默认: oms.json)')
    
    args = parser.parse_args()
    
    # 生成配置
    config = generate_oms_json(args.num_sessions, args.gw_type)
    
    # 写入文件
    with open(args.output, 'w', encoding='utf-8') as f:
        json.dump(config, f, indent=4, ensure_ascii=False)
    
    print(f"已生成 {args.num_sessions} 个session的oms.json文件到 {args.output}")

if __name__ == "__main__":
    main()