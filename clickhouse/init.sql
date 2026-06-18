-- 古代水碓凸轮机构动力学仿真数据库初始化脚本
-- ClickHouse 23.x+

CREATE DATABASE IF NOT EXISTS shuidui
ENGINE = Atomic;

USE shuidui;

-- 传感器原始数据表：每分钟上报一次
CREATE TABLE IF NOT EXISTS sensor_data (
    device_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    cam_angle Float64,
    duitou_acceleration Float64,
    grain_reaction_force Float64,
    frame_vibration_x Float64,
    frame_vibration_y Float64,
    frame_vibration_z Float64,
    frame_vibration_total Float64,
    water_wheel_speed Float64,
    duitou_position Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 1 YEAR
COMMENT '水碓传感器原始数据表';

-- 动力学仿真结果表
CREATE TABLE IF NOT EXISTS dynamics_simulation (
    device_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    cam_angle Float64,
    pounding_force Float64,
    impact_energy Float64,
    duitou_velocity Float64,
    duitou_displacement Float64,
    contact_time Float64,
    restitution_coefficient Float64,
    friction_force Float64
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 1 YEAR
COMMENT '凸轮动力学仿真结果表';

-- 告警记录表
CREATE TABLE IF NOT EXISTS alerts (
    id UUID,
    device_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    alert_type String,
    alert_level String,
    alert_message String,
    alert_value Float64,
    threshold Float64,
    acknowledged Bool DEFAULT false
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp, alert_type)
TTL timestamp + INTERVAL 1 YEAR
COMMENT '告警记录表';

-- 舂捣力优化结果表
CREATE TABLE IF NOT EXISTS optimization_results (
    id UUID,
    device_id String,
    timestamp DateTime64(3, 'Asia/Shanghai'),
    cam_base_radius Float64,
    cam_lift Float64,
    cam_pressure_angle Float64,
    cam_profile_type String,
    target_efficiency Float64,
    actual_efficiency Float64,
    average_pounding_force Float64,
    impact_energy_per_cycle Float64,
    husking_rate Float64,
    grain_breakage_rate Float64,
    optimization_parameters String
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 2 YEAR
COMMENT '舂捣力优化结果表';

-- 设备元数据表
CREATE TABLE IF NOT EXISTS devices (
    device_id String,
    device_name String,
    location String,
    installation_date Date,
    cam_base_radius Float64,
    cam_lift Float64,
    duitou_mass Float64,
    water_flow_rate Float64,
    frame_vibration_threshold Float64,
    is_active Bool DEFAULT true
)
ENGINE = ReplacingMergeTree()
ORDER BY device_id
COMMENT '水碓设备元数据表';

-- 插入默认设备数据
INSERT INTO devices (device_id, device_name, location, installation_date, cam_base_radius, cam_lift, duitou_mass, water_flow_rate, frame_vibration_threshold, is_active) VALUES
('shuidui-001', '汉代一号水碓', '河南南阳考古现场', '2024-03-15', 0.15, 0.12, 25.0, 0.05, 5.0, true),
('shuidui-002', '汉代二号水碓', '陕西西安考古现场', '2024-06-20', 0.18, 0.15, 32.0, 0.06, 5.0, true),
('shuidui-003', '汉代三号水碓', '四川成都考古现场', '2024-09-10', 0.12, 0.10, 20.0, 0.04, 5.0, true);

-- 创建物化视图：每分钟统计
CREATE MATERIALIZED VIEW IF NOT EXISTS sensor_stats_1min
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
AS
SELECT
    device_id,
    toStartOfMinute(timestamp) AS timestamp,
    count() AS sample_count,
    avg(cam_angle) AS avg_cam_angle,
    max(cam_angle) AS max_cam_angle,
    avg(duitou_acceleration) AS avg_acceleration,
    max(abs(duitou_acceleration)) AS max_acceleration,
    avg(grain_reaction_force) AS avg_grain_force,
    max(grain_reaction_force) AS max_grain_force,
    avg(frame_vibration_total) AS avg_vibration,
    max(frame_vibration_total) AS max_vibration
FROM sensor_data
GROUP BY device_id, timestamp;

-- 创建告警触发器视图
CREATE MATERIALIZED VIEW IF NOT EXISTS vibration_alerts_mv
TO alerts
AS
SELECT
    generateUUIDv4() AS id,
    device_id,
    timestamp,
    'frame_vibration' AS alert_type,
    CASE
        WHEN frame_vibration_total > 8.0 THEN 'critical'
        WHEN frame_vibration_total > 5.0 THEN 'warning'
        ELSE 'info'
    END AS alert_level,
    CASE
        WHEN frame_vibration_total > 8.0 THEN '机架振动严重超限，请立即停机检查！'
        WHEN frame_vibration_total > 5.0 THEN '机架振动超过预警阈值'
        ELSE '振动正常'
    END AS alert_message,
    frame_vibration_total AS alert_value,
    5.0 AS threshold,
    false AS acknowledged
FROM sensor_data
WHERE frame_vibration_total > 5.0;
