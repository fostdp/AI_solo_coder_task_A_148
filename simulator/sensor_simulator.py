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

import argparse
import json
import math
import random
import time
from datetime import datetime, timezone

try:
    import paho.mqtt.client as mqtt
except ImportError:
    print("请安装 paho-mqtt: pip install paho-mqtt")
    exit(1)

MQTT_TOPIC = "shuidui/sensor"

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

GRAIN_PARAMS = {
    "rice": {
        "grain_reaction_coeff": 0.85,
        "breakage_threshold": 1200.0,
        "moisture": 0.15,
    },
    "millet": {
        "grain_reaction_coeff": 0.65,
        "breakage_threshold": 800.0,
        "moisture": 0.12,
    },
    "wheat": {
        "grain_reaction_coeff": 0.75,
        "breakage_threshold": 1000.0,
        "moisture": 0.13,
    },
}

CAM_PROFILES = ["harmonic", "cycloidal", "trapezoidal", "polynomial"]


class WaterTreadmillSimulator:
    def __init__(self, device_config, cam_profile="harmonic", grain_params=None):
        self.device_id = device_config["device_id"]
        self.cam_base_radius = device_config["cam_base_radius"]
        self.cam_lift = device_config["cam_lift"]
        self.duitou_mass = device_config["duitou_mass"]
        self.base_speed = device_config["base_speed"]
        self.water_flow_rate = device_config["water_flow_rate"]

        self.cam_profile = cam_profile
        self.grain_params = grain_params or GRAIN_PARAMS["rice"]

        self.cam_angle = 0.0
        self.cycle_count = 0
        self.grain_level = 1.0
        self.stall_probability = 0.001
        self.is_stalled = False
        self.stall_timer = 0

        self._lift_method = {
            "harmonic": self.harmonic_lift,
            "cycloidal": self.cycloidal_lift,
            "trapezoidal": self.trapezoidal_lift,
            "polynomial": self.polynomial_lift,
        }[cam_profile]

    def harmonic_lift(self, t, h):
        if t < 0.5:
            s = h * (1 - math.cos(math.pi * t)) / 2
            v = h * math.pi * math.sin(math.pi * t) / 2
            a = h * math.pi ** 2 * math.cos(math.pi * t) / 2
        else:
            s = h * (1 + math.cos(math.pi * t)) / 2
            v = -h * math.pi * math.sin(math.pi * t) / 2
            a = -h * math.pi ** 2 * math.cos(math.pi * t) / 2
        return s, v, a

    def cycloidal_lift(self, t, h):
        if t < 0.5:
            s = h * (t - math.sin(2 * math.pi * t) / (2 * math.pi))
            v = h * (1 - math.cos(2 * math.pi * t))
            a = h * 2 * math.pi * math.sin(2 * math.pi * t)
        else:
            s = h * (1 - ((1 - t) - math.sin(2 * math.pi * (1 - t)) / (2 * math.pi)))
            v = h * (1 - math.cos(2 * math.pi * (1 - t)))
            a = -h * 2 * math.pi * math.sin(2 * math.pi * (1 - t))
        return s, v, a

    def trapezoidal_lift(self, t, h):
        ta = 1.0 / 3.0
        tb = 2.0 / 3.0
        amax = 6.0 * h
        if t < 0.5:
            if t < ta:
                s = 0.5 * amax * t ** 2
                v = amax * t
                a = amax
            elif t < tb:
                s = 0.5 * amax * ta ** 2 + amax * ta * (t - ta)
                v = amax * ta
                a = 0.0
            else:
                v_const = amax * ta
                s_at_tb = 0.5 * amax * ta ** 2 + amax * ta * (tb - ta)
                s = s_at_tb + v_const * (t - tb) - 0.5 * amax * (t - tb) ** 2
                v = v_const - amax * (t - tb)
                a = -amax
        else:
            tm = 1.0 - t
            if tm < ta:
                s_rem = 0.5 * amax * tm ** 2
                s = h - s_rem
                v = -amax * tm
                a = -amax
            elif tm < tb:
                s_rem = 0.5 * amax * ta ** 2 + amax * ta * (tm - ta)
                s = h - s_rem
                v = -amax * ta
                a = 0.0
            else:
                v_const = amax * ta
                s_at_tb = 0.5 * amax * ta ** 2 + amax * ta * (tb - ta)
                s_rem = s_at_tb + v_const * (tm - tb) - 0.5 * amax * (tm - tb) ** 2
                s = h - s_rem
                v = -(v_const - amax * (tm - tb))
                a = amax
        return s, v, a

    def polynomial_lift(self, t, h):
        if t < 0.5:
            s = h * (10 * t ** 3 - 15 * t ** 4 + 6 * t ** 5)
            v = h * (30 * t ** 2 - 60 * t ** 3 + 30 * t ** 4)
            a = h * (60 * t - 180 * t ** 2 + 120 * t ** 3)
        else:
            tm = 1.0 - t
            s_rise = h * (10 * t ** 3 - 15 * t ** 4 + 6 * t ** 5)
            s = s_rise
            v = h * (30 * t ** 2 - 60 * t ** 3 + 30 * t ** 4)
            a = h * (60 * t - 180 * t ** 2 + 120 * t ** 3)
        return s, v, a

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
        else:
            t = angle_rad / math.pi

        h = self.cam_lift
        lift, velocity, acceleration = self._lift_method(t, h)
        velocity *= self.base_speed
        acceleration *= self.base_speed ** 2

        noise_acc = random.gauss(0, 0.5)
        total_acceleration = acceleration + noise_acc

        grain_reaction_coeff = self.grain_params["grain_reaction_coeff"]
        moisture = self.grain_params["moisture"]
        breakage_threshold = self.grain_params["breakage_threshold"]

        moisture_noise = 1.0 + random.gauss(0, moisture * 0.1)

        grain_force = 0.0
        if lift < 0.02 and velocity > -0.05 and velocity < 0.1:
            impact_velocity = max(0, -velocity)
            impact_force = self.duitou_mass * impact_velocity * 20
            grain_force = impact_force * self.grain_level * grain_reaction_coeff * moisture_noise
        elif lift > 0.01:
            grain_force = 0.0
        else:
            grain_force = self.duitou_mass * 9.81 * self.grain_level * 0.3 * grain_reaction_coeff * moisture_noise

        if grain_force > breakage_threshold:
            grain_force = breakage_threshold

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


def parse_args():
    parser = argparse.ArgumentParser(description="水碓传感器模拟器")
    parser.add_argument(
        "--cam-profile",
        choices=CAM_PROFILES,
        default="harmonic",
        help="Cam motion law (default: harmonic)",
    )
    parser.add_argument(
        "--grain",
        choices=["rice", "millet", "wheat"],
        default="rice",
        help="Grain type (default: rice)",
    )
    parser.add_argument(
        "--interval",
        type=float,
        default=1.0,
        help="Publish interval in seconds (default: 1.0)",
    )
    parser.add_argument(
        "--broker",
        type=str,
        default="localhost",
        help="MQTT broker address (default: localhost)",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=1883,
        help="MQTT broker port (default: 1883)",
    )
    return parser.parse_args()


def main():
    args = parse_args()

    grain_params = GRAIN_PARAMS[args.grain]

    print("=" * 60)
    print("水碓传感器模拟器")
    print("=" * 60)
    print(f"Cam Profile: {args.cam_profile}")
    print(f"Grain Type: {args.grain}")
    print(f"MQTT Broker: {args.broker}:{args.port}")
    print(f"Topic: {MQTT_TOPIC}")
    print(f"设备数量: {len(DEVICES)}")
    print(f"上报间隔: {args.interval} 秒")
    print("=" * 60)

    client = mqtt.Client("shuidui-simulator")
    client.on_connect = on_connect
    client.on_publish = on_publish

    try:
        client.connect(args.broker, args.port, 60)
        client.loop_start()
    except Exception as e:
        print(f"无法连接到MQTT Broker: {e}")
        print("请确保已启动MQTT服务器（如 Mosquitto）")
        print("或者使用 Docker: docker run -d -p 1883:1883 eclipse-mosquitto")
        return

    simulators = [
        WaterTreadmillSimulator(dev, cam_profile=args.cam_profile, grain_params=grain_params)
        for dev in DEVICES
    ]

    last_minute_print = 0

    try:
        while True:
            for sim in simulators:
                data = sim.step(args.interval)
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

            time.sleep(args.interval)

    except KeyboardInterrupt:
        print("\n模拟器已停止")
    finally:
        client.loop_stop()
        client.disconnect()


if __name__ == "__main__":
    main()
