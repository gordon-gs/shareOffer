#!/usr/bin/env python3

import json
import subprocess
import argparse
import os

def load_config(config_file):
    """从JSON文件中加载配置"""
    with open(config_file, 'r') as f:
        return json.load(f)

def run_script(script_path, args):
    """运行指定的脚本"""
    cmd = ['python3', script_path] + args
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode != 0:
        print(f"Error running {script_path}: {result.stderr.decode('utf-8')}")
        return False
    return True

def main():
    parser = argparse.ArgumentParser(description='Generate all config files from JSON config')
    parser.add_argument('--config', default='gen_config_param.json', help='Config file path (default: gen_config_param.json)')
    
    args = parser.parse_args()
    
    # 加载配置文件
    config = load_config(args.config)
    
    # 生成oms.json
    oms_args = [
        '--num-sessions', str(config['oms']['num_sessions']),
        '--gw-type', str(config['oms']['gw_type']),
        '--output', config['oms']['output_file']
    ]
    if not run_script('generate_oms_json.py', oms_args):
        print("Failed to generate oms.json")
        return
    
    # 生成tdgw.json
    tdgw_args = [
        '--num-sessions', str(config['tdgw']['num_sessions']),
        '--start-port', str(config['tdgw']['start_port']),
        '--output', config['tdgw']['output_file']
    ]
    if not run_script('generate_tdgw_json.py', tdgw_args):
        print("Failed to generate tdgw.json")
        return
    
    # 生成tgw.json
    tgw_args = [
        '--num-sessions', str(config['tgw']['num_sessions']),
        '--target-comp-id-start', str(config['tgw']['target-comp-id-start']),
        '--platform-id', str(config['tgw']['platform-id']), 
        '--socket-connect-port', str(config['tgw']['socket-connect-port']),
        '--socket-connect-host', str(config['tgw']['socket-connect-host']),
        '--password', str(config['tgw']['password']),
        '--output', config['tgw']['output_file']
    ]
    if not run_script('generate_tgw_json.py', tgw_args):
        print("Failed to generate tgw.json")
        return
    
    # 生成tcp_share_config.json
    tcp_args = [
        '--servers', str(config['tcp_share_config']['servers']),
        '--clients', str(config['tcp_share_config']['clients']),
        '--start-server-port', str(config['tcp_share_config']['start_server_port']),
        '--start-client-port', str(config['tcp_share_config']['start_client_port']),
        '--start-conn-id', str(config['tcp_share_config']['start_conn_id']),
         '--gw-type', str(config['tcp_share_config']['gw_type'])
    ]
    if not run_script('generate_tcp_share_config.py', tcp_args):
        print("Failed to generate tcp_share_config.json")
        return
    
    print("All config files generated successfully!")

if __name__ == "__main__":
    main()