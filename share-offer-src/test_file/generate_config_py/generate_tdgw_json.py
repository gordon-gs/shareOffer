import json
import argparse

def generate_json(num_sessions, start_port):
    # 固定配置
    config = {
        "reconnect_interval": 3,
        "heart_bt_int": 3,
        "prtcl_version": "1.11",
        "session": []
    }
    
    # 生成session列表
    for i in range(num_sessions):
        session = {
            "sender_comp_id": "CICC-SO01",
            "target_comp_id": f"TDGW-{i+2:02d}",
            "pbus": ["50054"],
            "platform_id": 0,
            "socket_connect_port": start_port + i,
            "socket_connect_host": "127.0.0.1"
        }
        config["session"].append(session)
    
    return config

def main():
    parser = argparse.ArgumentParser(description='Generate JSON configuration file')
    parser.add_argument('--num-sessions', type=int, default=9, help='Number of sessions to generate (default: 9)')
    parser.add_argument('--start-port', type=int, default=18009, help='Starting port number (default: 18009)')
    parser.add_argument('--output', default='tdgw_generated.json', help='Output file name (default: tdgw.json)')
    
    args = parser.parse_args()
    
    # 生成配置
    config = generate_json(args.num_sessions, args.start_port)
    
    # 写入文件
    with open(args.output, 'w') as f:
        json.dump(config, f, indent=4)
    
    print(f"Generated {args.num_sessions} sessions with starting port {args.start_port}")
    print(f"Output written to {args.output}")

if __name__ == "__main__":
    main()