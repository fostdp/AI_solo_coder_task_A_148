-- ============================================================
-- 降采样与数据保留策略脚本
-- 适用于 shuidui 数据库，ClickHouse 23.x+
-- 本脚本在 init.sql 基础上追加：
--   1. 创建降采样物化视图（5分钟 / 小时粒度）
--   2. 修改现有原始表的 TTL，实现冷热数据分层
-- ============================================================

USE shuidui;

-- ============================================================
-- 第一部分：动力学仿真 5 分钟降采样
-- 将 dynamics_simulation 原始数据聚合为 5 分钟统计，
-- 包含 pounding_force / impact_energy / duitou_velocity 的
-- 均值、最小值、最大值，以及 friction_force 和
-- restitution_coefficient 的均值。
-- 保留 2 年，用于中长期趋势分析。
-- ============================================================

CREATE MATERIALIZED VIEW IF NOT EXISTS dynamics_stats_5min
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 2 YEAR
AS
SELECT
    device_id,
    toStartOfFiveMinutes(timestamp) AS timestamp,
    avg(pounding_force)       AS avg_pounding_force,
    min(pounding_force)       AS min_pounding_force,
    max(pounding_force)       AS max_pounding_force,
    avg(impact_energy)        AS avg_impact_energy,
    min(impact_energy)        AS min_impact_energy,
    max(impact_energy)        AS max_impact_energy,
    avg(duitou_velocity)      AS avg_duitou_velocity,
    min(duitou_velocity)      AS min_duitou_velocity,
    max(duitou_velocity)      AS max_duitou_velocity,
    avg(friction_force)       AS avg_friction_force,
    avg(restitution_coefficient) AS avg_restitution_coefficient
FROM dynamics_simulation
GROUP BY device_id, timestamp;

-- ============================================================
-- 第二部分：优化结果小时降采样
-- 将 optimization_results 聚合为小时级统计，
-- 包含实际效率均值、平均舂捣力均值和优化次数。
-- 保留 3 年，用于长期效能评估与参数调优回顾。
-- ============================================================

CREATE MATERIALIZED VIEW IF NOT EXISTS optimization_stats_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 3 YEAR
AS
SELECT
    device_id,
    toStartOfHour(timestamp) AS timestamp,
    avg(actual_efficiency)       AS avg_actual_efficiency,
    avg(average_pounding_force)  AS avg_average_pounding_force,
    count()                      AS optimization_count
FROM optimization_results
GROUP BY device_id, timestamp;

-- ============================================================
-- 第三部分：传感器数据 5 分钟降采样
-- 将 sensor_data 原始数据聚合为 5 分钟统计，
-- 包含 cam_angle 均值/最小/最大、vibration 均值/最大、
-- acceleration 均值/最大、grain_force 均值、采样计数。
-- 保留 1 年，补充已有的 1 分钟聚合视图。
-- ============================================================

CREATE MATERIALIZED VIEW IF NOT EXISTS sensor_stats_5min
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (device_id, timestamp)
TTL timestamp + INTERVAL 1 YEAR
AS
SELECT
    device_id,
    toStartOfFiveMinutes(timestamp) AS timestamp,
    avg(cam_angle)                AS avg_cam_angle,
    min(cam_angle)                AS min_cam_angle,
    max(cam_angle)                AS max_cam_angle,
    avg(frame_vibration_total)    AS avg_vibration,
    max(frame_vibration_total)    AS max_vibration,
    avg(duitou_acceleration)      AS avg_acceleration,
    max(abs(duitou_acceleration)) AS max_acceleration,
    avg(grain_reaction_force)     AS avg_grain_force,
    count()                       AS sample_count
FROM sensor_data
GROUP BY device_id, timestamp;

-- ============================================================
-- 第四部分：修改现有原始表的 TTL 策略
-- 缩短原始数据保留周期，依赖降采样聚合表提供历史查询。
-- ============================================================

-- 传感器原始数据：热数据仅保留 30 天，历史查询使用 1 分钟聚合（sensor_stats_1min）
ALTER TABLE sensor_data MODIFY TTL timestamp + INTERVAL 30 DAY;

-- 动力学仿真原始数据：保留 90 天，历史查询使用 5 分钟聚合（dynamics_stats_5min）
ALTER TABLE dynamics_simulation MODIFY TTL timestamp + INTERVAL 90 DAY;

-- 告警记录：保留 6 个月
ALTER TABLE alerts MODIFY TTL timestamp + INTERVAL 6 MONTH;
