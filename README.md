# 🏛️ 古代水碓凸轮机构动力学仿真与舂捣力优化系统

基于 Rust + ClickHouse + Three.js 的全栈工业仿真系统，模拟汉代水碓的凸轮动力学、舂捣力优化与实时告警。

## 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        Docker Compose                           │
│                                                                  │
│  ┌──────────────┐   MQTT    ┌──────────────┐                    │
│  │  simulator   │──────────▶│  mosquitto   │                    │
│  │ (传感器模拟器) │          │  (MQTT Broker)│                    │
│  └──────────────┘           └──────┬───────┘                    │
│                                    │                             │
│                              ┌─────▼──────┐                     │
│                              │  backend   │                     │
│                              │  (Rust)    │                     │
│                              │            │                     │
│                              │ ┌────────┐ │   WebSocket        │
│                              │ │mqtt_   │ │──────────┐         │
│                              │ │receiver│ │          │         │
│                              │ └───┬────┘ │          │         │
│                              │     │mpsc  │          │         │
│                              │ ┌───▼────┐ │          │         │
│                              │ │cam_    │ │          │         │
│                              │ │simulator│ │         │         │
│                              │ └───┬────┘ │          │         │
│                              │     │mpsc  │          │         │
│                              │ ┌───▼────┐ │          │         │
│                              │ │force_  │ │          │         │
│                              │ │optimizer│ │         │         │
│                              │ └───┬────┘ │          │         │
│                              │     │mpsc  │          │         │
│                              │ ┌───▼────┐ │          │         │
│                              │ │alarm_ws│◀│──────────┘         │
│                              │ └───┬────┘ │                    │
│                              │     │      │  :8080             │
│                              │  Prometheus  │ /metrics          │
│                              └─────┬──────┘                    │
│                                    │                             │
│                              ┌─────▼──────┐                     │
│                              │ clickhouse │                     │
│                              │  (时序数据库) │                    │
│                              └────────────┘                     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

         ┌──────────────────────────┐
         │       浏览器前端          │
         │  ┌─────────┐ ┌─────────┐ │
         │  │trip_    │ │cam_     │ │
         │  │hammer_3d│ │panel    │ │
         │  │(Three.js)│ │(2D图表) │ │
         │  └─────────┘ └─────────┘ │
         └──────────────────────────┘
```

## 模块说明

### Rust 后端（4 大模块，mpsc channel 通信）

| 模块 | 文件 | 职责 |
|------|------|------|
| **mqtt_receiver** | `backend/src/mqtt_receiver.rs` | MQTT 数据采集、传感器校验、分发 |
| **cam_simulator** | `backend/src/cam_simulator.rs` | 多体动力学仿真、Hertz 接触碰撞、罚函数刚度自适应 |
| **force_optimizer** | `backend/src/force_optimizer.rs` | 凸轮曲线网格搜索优化、公差分析、脱壳效率 |
| **alarm_ws** | `backend/src/alarm_ws.rs` | 告警检测（振动/加速度/卡死）、WebSocket 推送、去重限流 |

### 前端（2 个 JS 模块）

| 模块 | 文件 | 职责 |
|------|------|------|
| **trip_hammer_3d.js** | `frontend/trip_hammer_3d.js` | Three.js 三维渲染、GPU Shader 顶点变形 |
| **cam_panel.js** | `frontend/cam_panel.js` | 凸轮轮廓 2D 绘制、舂捣力柱状图、控制面板 |

### 外置配置

| 配置 | 文件 | 内容 |
|------|------|------|
| 动力学参数 | `backend/config/dynamics_config.json` | 重力、恢复系数、赫兹刚度、阻尼比、谷物参数 |
| 优化参数 | `backend/config/optimization_config.json` | 公差权重、网格步数、评分权重、可行性阈值 |

## 部署步骤

### 前置要求

- Docker 20.10+
- Docker Compose V2

### 一键启动

```bash
git clone <repo-url> && cd shuidui
docker compose up -d
```

等待所有服务健康后，访问：

- 前端界面：http://localhost:8080
- API 接口：http://localhost:8080/api/devices
- Prometheus 指标：http://localhost:8080/metrics
- ClickHouse 控制台：http://localhost:8123/play

### 查看日志

```bash
# 后端日志（tracing JSON 格式）
docker compose logs -f backend

# 模拟器日志
docker compose logs -f simulator
```

### 停止服务

```bash
docker compose down -v
```

## 传感器模拟器用法

### Docker Compose 方式

修改 `docker-compose.yml` 中 simulator 的环境变量：

```yaml
simulator:
  environment:
    CAM_PROFILE: cycloidal    # 凸轮曲线类型
    GRAIN_TYPE: wheat         # 谷物种类
    INTERVAL: "0.5"           # 上报间隔（秒）
```

重启：`docker compose up -d simulator`

### 本地运行

```bash
pip install paho-mqtt
cd simulator

# 默认参数：简谐凸轮 + 水稻
python sensor_simulator.py

# 摆线凸轮 + 小米，1秒间隔
python sensor_simulator.py --cam-profile cycloidal --grain millet --interval 1.0

# 多项式凸轮 + 小麦，连远程 Broker
python sensor_simulator.py --cam-profile polynomial --grain wheat --broker 192.168.1.100
```

### 凸轮曲线类型

| 类型 | 说明 | 运动特征 |
|------|------|----------|
| `harmonic` | 简谐运动 | 加速度按余弦变化，柔性冲击 |
| `cycloidal` | 摆线运动 | 加速度按正弦变化，无刚性冲击 |
| `trapezoidal` | 梯形加速度 | 分段线性加速度，工程常用 |
| `polynomial` | 3-4-5 多项式 | 高阶平滑，Jerk 连续 |

### 谷物种类

| 类型 | 脱壳系数 | 破碎阈值(N) | 含水率 |
|------|----------|-------------|--------|
| `rice` | 0.85 | 1200 | 15% |
| `millet` | 0.65 | 800 | 12% |
| `wheat` | 0.75 | 1000 | 13% |

## 可观测性

### Tracing

后端使用 `tracing` + `tracing-subscriber`，输出 JSON 格式结构化日志：

```bash
# 调整日志级别
RUST_LOG=debug docker compose up backend
```

### Prometheus 指标

访问 `http://localhost:8080/metrics` 获取 Prometheus 格式指标：

| 指标 | 类型 | 说明 |
|------|------|------|
| `mqtt_messages_received_total` | Counter | MQTT 消息接收总数 |
| `mqtt_messages_invalid_total` | Counter | 无效消息数 |
| `simulations_run_total` | Counter | 仿真执行总数 |
| `optimizations_run_total` | Counter | 优化执行总数 |
| `alerts_generated_total` | Counter | 告警生成数 |
| `alerts_suppressed_total` | Counter | 去重抑制数 |
| `active_devices` | Gauge | 活跃设备数 |
| `websocket_connections` | Gauge | WebSocket 连接数 |
| `simulation_duration_seconds` | Histogram | 仿真耗时分布 |
| `optimization_duration_seconds` | Histogram | 优化耗时分布 |
| `pounding_force_newtons` | Histogram | 舂捣力分布 |

## ClickHouse 数据保留策略

| 表 | 原始 TTL | 降采样 |
|----|----------|--------|
| sensor_data | 30 天 | 1 分钟聚合 / 5 分钟聚合 |
| dynamics_simulation | 90 天 | 5 分钟聚合（保留 2 年） |
| alerts | 6 个月 | — |
| optimization_results | 2 年 | 小时聚合（保留 3 年） |

## API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/api/devices` | 设备列表 |
| GET | `/api/device/{id}` | 设备详情 |
| GET | `/api/sensor-data?device_id=&limit=` | 传感器数据 |
| GET | `/api/dynamics?device_id=&limit=` | 动力学结果 |
| GET | `/api/alerts?device_id=&limit=` | 告警列表 |
| GET | `/api/cam-profile/{id}?base_radius=&lift=` | 凸轮轮廓生成 |
| POST | `/api/simulate` | 执行动力学仿真 |
| POST | `/api/optimize` | 执行凸轮优化 |
| GET | `/metrics` | Prometheus 指标 |
| WS | `/ws/alerts` | 告警 WebSocket |
