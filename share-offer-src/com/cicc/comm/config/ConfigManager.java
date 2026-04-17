package com.cicc.comm.config;

import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import com.cicc.utils.CommUtils;

import cn.hutool.core.lang.Singleton;
import cn.hutool.setting.Setting;
/**
 * @author guanzl
 */
public class ConfigManager {
	private static final Logger log = LoggerFactory.getLogger(ConfigManager.class);

	private static Setting setting;
	private static CommConfig comm;

	synchronized public static void load(String fileName) {
		if (fileName == null) {
			fileName = "config/systemConfig.setting";
		}
		if (setting == null) {

			try {
				setting = new Setting(fileName);
				comm = get(CommConfig.class, "comm");
				comm.setIp(CommUtils.getIP());
				comm.setAddressKey("/" + comm.getIp() + ":" + (comm.getPort() == 0 ? "" : comm.getPort()) + " " + (comm.getPort_ssl() == 0 ? "" : comm.getPort_ssl()));
				log.info("√ load {}, localhost:{}", fileName, comm.getAddressKey());
			} catch (Exception e) {
				log.error("X load {} fail!", fileName);
				e.printStackTrace();
				System.exit(1);
			}
		}
	}

	public static <T> T get(Class<T> cls, String name) {
		T obj = Singleton.get(cls);
		setting.getSetting(name).toBean(obj);
		return obj;
	}

	public static Setting getSetting() {
		return setting;
	}

	public static CommConfig getComm() {
		return comm;
	}
}
