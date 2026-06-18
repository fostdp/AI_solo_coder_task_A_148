#!/usr/bin/env python3
"""
水碓传感器模拟器
模拟汉代水碓的传感器数据，通过MQTT每分钟上报一次

数据包括：
- 凸轮转角
- 碓头加速度
- 谷物反力
- 机架振动 (x, y, z)
- 水轮转速
- 碓头位置
"""

import json
import time
import random
import math
from datetime import datetime, timezone

try:
    import paho.mqtt.client as mqtt
except ImportError:
    print("请安装 paho-mqtt: pip install paho-mqtt")
    exit(1)

MQTT_BROKER = "localhost"
MQTT_PORT = 1883
MQTT_TOPIC = "shuidui/sensor"

SLEEP_INTERVAL = 1

DEVICES = [
    {
        "device_id": "shuidui-001",
        "device_name": "汉代一号水碓",
        "cam_base_radius": 0.15,
        "cam_lift": 0.12,
        "duitou_mass": 25.0,
        "water_flow_rate": 0.05,
        "base_speed": 3.0,
    },
    {
        "device_id": "shuidui-002",
        "device_name": "汉代二号水碓",
        "cam_base_radius": 0.18,
        "cam_lift": 0.15,
        "duitou_mass": 32.0,
        "water_flow_rate": 0.06,
        "base_speed": 2.5,
    },
    {
        "device_id": "shuidui-003",
        "device_name": "汉代三号水碓",
        "cam_base_radius": 0.12,
        "cam_lift": 0.10,
        "duitou_mass": 20.0,
        "water_flow_rate": 0.04,
        "base_speed": 3.5,
    },
]


class WaterTreadmillSimulator:
    def __init__(self, device_config):
        self.device_id = device_config["device_id"]
        self.cam_base_radius = device_config["cam_base_radius"]
        self.cam_lift = device_config["cam_lift"]
        self.duitou_mass = device_config["duitou_mass"]
        self.base_speed = device_config["base_speed"]
        self.water_flow_rate = device_config["water_flow_rate"]

        self.cam_angle = 0.0
        self.cycle_count = 0
        self.grain_level = 1.0
        self.stall_probability = 0.001
        self.is_stalled = False
        self.stall_timer = 0

    def step(self, dt):
        if self.is_stalled:
            self.stall_timer -= dt
            if self.stall_timer <= 0:
                self.is_stalled = False
                print(f"[{self.device_id}] 碓头已恢复运行")
            return self.generate_sensor_data()

        if random.random() < self.stall_probability * dt:
            self.is_stalled = True
            self.stall_timer = random.uniform(2, 8)
            print(f"[{self.device_id}] 警告：碓头可能卡死！")

        speed_variation = random.gauss(1.0, 0.05)
        angular_velocity = self.base_speed * speed_variation

        self.cam_angle += angular_velocity * dt
        if self.cam_angle >= 360.0:
            self.cam_angle -= 360.0
            self.cycle_count += 1
            self.grain_level = max(0.1, self.grain_level - 0.001)

        return self.generate_sensor_data()

    def generate_sensor_data(self):
        angle_rad = math.radians(self.cam_angle)

        if angle_rad < math.pi:
            t = angle_rad / math.pi
            lift = self.cam_lift * (1 - math.cos(math.pi * t)) / 2
            velocity = self.cam_lift * math.pi * math.sin(math.pi * t) / 2 * self.base_speed
            acceleration = self.cam_lift * math.pi ** 2 * math.cos(math.pi * t) / 2 * self.base_speed ** 2
        else:
            t = (angle_rad - math.pi) / math.pi
            lift = self.cam_lift * (1 + math.cos(math.pi * t)) / 2
            velocity = -self.cam_lift * math.pi * math.sin(math.pi * t) / 2 * self.base_speed
            acceleration = -self.cam_lift * math.pi ** 2 * math.cos(math.pi * t) / 2 * self.base_speed ** 2

        noise_acc = random.gauss(0, 0.5)
        total_acceleration = acceleration + noise_acc

        grain_force = 0.0
        if lift < 0.02 and velocity > -0.05 and velocity < 0.1:
            impact_velocity = max(0, -velocity)
            impact_force = self.duitou_mass * impact_velocity * 20
            grain_force = impact_force * self.grain_level
        elif lift > 0.01:
            grain_force = 0.0
        else:
            grain_force = self.duitou_mass * 9.81 * self.grain_level * 0.3

        vib_base = 0.5
        vib_impact_factor = 1 + abs(total_acceleration) * 0.05
        vib_freq_factor = abs(math.sin(angle_rad * 3))

        vibration_x = random.gauss(vib_base * vib_impact_factor * vib_freq_factor, 0.1)
        vibration_y = random.gauss(vib_base * vib_impact_factor * 0.8, 0.08)
        vibration_z = random.gauss(vib_base * vib_impact_factor * 0.5, 0.05)

        water_wheel_speed = self.base_speed if not self.is_stalled else 0.01

        return {
            "device_id": self.device_id,
            "timestamp": int(datetime.now(timezone.utc).timestamp() * 1000),
            "cam_angle": round(self.cam_angle, 4),
            "duitou_acceleration": round(total_acceleration, 4),
            "grain_reaction_force": round(grain_force, 2),
            "frame_vibration_x": round(vibration_x, 4),
            "frame_vibration_y": round(vibration_y, 4),
            "frame_vibration_z": round(vibration_z, 4),
            "water_wheel_speed": round(water_wheel_speed, 4),
            "duitou_position": round(lift, 6),
        }


def on_connect(client, userdata, flags, rc):
    if rc == 0:
        print("MQTT 连接成功")
    else:
        print(f"MQTT 连接失败，错误码: {rc}")


def on_publish(client, userdata, mid):
    pass


def main():
    print("=" * 60)
    print("水碓传感器模拟器")
    print("=" * 60)
    print(f"MQTT Broker: {MQTT_BROKER}:{MQTT_PORT}")
    print(f"Topic: {MQTT_TOPIC}")
    print(f"设备数量: {len(DEVICES)}")
    print(f"上报间隔: {SLEEP_INTERVAL} 秒")
    print("=" * 60)

    client = mqtt.Client("shuidui-simulator")
    client.on_connect = on_connect
    client.on_publish = on_publish

    try:
        client.connect(MQTT_BROKER, MQTT_PORT, 60)
        client.loop_start()
    except Exception as e:
        print(f"无法连接到MQTT Broker: {e}")
        print("请确保已启动MQTT服务器（如 Mosquitto）")
        print("或者使用 Docker: docker run -d -p 1883:1883 eclipse-mosquitto")
        return

    simulators = [WaterTreadmillSimulator(dev) for dev in DEVICES]

    last_minute_print = 0

    try:
        while True:
            for sim in simulators:
                data = sim.step(SLEEP_INTERVAL)
                topic = f"{MQTT_TOPIC}/{data['device_id']}"

                payload = json.dumps(data, ensure_ascii=False)
                client.publish(topic, payload, qos=1)

                if int(time.time()) // 60 > last_minute_print:
                    last_minute_print = int(time.time()) // 60
                    print(f"\n[{datetime.now().strftime('%Y-%m-%d %H:%M:%S')}] 数据上报中...")
                    print(f"  {sim.device_id}: 凸轮={data['cam_angle']:.1f}° "
                          f"加速度={data['duitou_acceleration']:.2f}m/s² "
                          f"舂捣力={data['grain_reaction_force']:.1f}N "
                          f"振动={math.sqrt(data['frame_vibration_x']**2 + data['frame_vibration_y']**2 + data['frame_vibration_z']**2):.2f}m/s²")

            time.sleep(SLEEP_INTERVAL)

    except KeyboardInterrupt:
        print("\n模拟器已停止")
    finally:
        client.loop_stop()
        client.disconnect()


if __name__ == "__main__":
    main()
