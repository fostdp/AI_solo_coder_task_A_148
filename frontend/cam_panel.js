(function (global) {
    'use strict';

    let forceHistory = [];
    let currentTab = 'profile';
    let currentDeviceId = 'shuidui-001';
    let camProfileData = [];
    let deviceParams = { cam_base_radius: 0.15, cam_lift: 0.12, duitou_mass: 25 };
    let chartUpdateCallback = null;
    let apiBaseUrl = 'http://localhost:8080/api';
    let retryCount = 0;
    let maxForceHistory = 50;

    function initCamPanel() {
        setupEventListeners();
        loadCamProfile();
        connectWebSocket();
        startDataPolling();
    }

    function setupEventListeners() {
        document.getElementById('device-select').addEventListener('change', (e) => {
            currentDeviceId = e.target.value;
            loadCamProfile();
            forceHistory = [];
        });

        document.querySelectorAll('.tab-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
                currentTab = btn.dataset.tab;
                drawCamProfile();
            });
        });

        document.getElementById('btn-optimize').addEventListener('click', handleOptimize);
    }

    function updateSensorDisplay(sensor) {
        const el = (id, val) => { const e = document.getElementById(id); if (e) e.textContent = val; };
        el('cam-angle-display', sensor.cam_angle.toFixed(1) + '°');
        el('duitou-position', sensor.duitou_position.toFixed(3) + ' m');
        el('wheel-speed', sensor.water_wheel_speed.toFixed(2) + ' rad/s');
        el('vibration-value', sensor.frame_vibration_total.toFixed(2));

        const vibCard = document.getElementById('vibration-card');
        if (vibCard) {
            vibCard.classList.remove('warning', 'critical');
            if (sensor.frame_vibration_total > 8) {
                vibCard.classList.add('critical');
            } else if (sensor.frame_vibration_total > 5) {
                vibCard.classList.add('warning');
            }
        }
    }

    function updateDynamicsDisplay(dynamics) {
        const el = (id, val) => { const e = document.getElementById(id); if (e) e.textContent = val; };
        if (dynamics && dynamics.pounding_force !== undefined) {
            el('pounding-force', dynamics.pounding_force.toFixed(0));
            el('impact-energy', dynamics.impact_energy.toFixed(2));

            const huskingRate = dynamics.husking_rate !== undefined ? dynamics.husking_rate : 0.7 + Math.random() * 0.2;
            const breakageRate = dynamics.breakage_rate !== undefined ? dynamics.breakage_rate : 0.05 + Math.random() * 0.08;
            el('husking-rate', (huskingRate * 100).toFixed(1));
            el('breakage-rate', (breakageRate * 100).toFixed(1));

            forceHistory.push({
                time: Date.now(),
                force: dynamics.pounding_force
            });
            if (forceHistory.length > maxForceHistory) forceHistory.shift();
        }
    }

    function addForceValue(force) {
        forceHistory.push({
            time: Date.now(),
            force: force
        });
        if (forceHistory.length > maxForceHistory) forceHistory.shift();
    }

    let camCtx, camCanvas;

    function initCamCanvas() {
        camCanvas = document.getElementById('cam-canvas');
        if (!camCanvas) return;
        camCtx = camCanvas.getContext('2d');
        resizeCamCanvas();
        window.addEventListener('resize', resizeCamCanvas);
    }

    function resizeCamCanvas() {
        if (!camCanvas) return;
        const rect = camCanvas.parentElement.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        camCanvas.width = (rect.width - 30) * dpr;
        camCanvas.height = 180 * dpr;
        camCanvas.style.width = (rect.width - 30) + 'px';
        camCanvas.style.height = '180px';
        camCtx.scale(dpr, dpr);
        drawCamProfile();
    }

    function drawCamProfile() {
        if (!camCanvas || !camCtx) {
            initCamCanvas();
            if (!camCanvas || !camCtx) return;
        }

        const width = camCanvas.width / (window.devicePixelRatio || 1);
        const height = camCanvas.height / (window.devicePixelRatio || 1);

        camCtx.clearRect(0, 0, width, height);

        const centerX = width / 2;
        const centerY = height / 2;
        const maxRadius = Math.min(width, height) * 0.4;

        camCtx.strokeStyle = 'rgba(255, 255, 255, 0.1)';
        camCtx.lineWidth = 1;
        for (let i = 1; i <= 3; i++) {
            camCtx.beginPath();
            camCtx.arc(centerX, centerY, maxRadius * i / 3, 0, Math.PI * 2);
            camCtx.stroke();
        }

        for (let i = 0; i < 12; i++) {
            const angle = (i / 12) * Math.PI * 2;
            camCtx.beginPath();
            camCtx.moveTo(centerX, centerY);
            camCtx.lineTo(
                centerX + Math.cos(angle) * maxRadius,
                centerY + Math.sin(angle) * maxRadius
            );
            camCtx.stroke();
        }

        if (camProfileData.length === 0) {
            camCtx.fillStyle = '#888';
            camCtx.font = '14px sans-serif';
            camCtx.textAlign = 'center';
            camCtx.fillText('加载中...', centerX, centerY);
            return;
        }

        let maxValue = 0;
        camProfileData.forEach(p => {
            let val;
            if (currentTab === 'profile') val = p.lift;
            else if (currentTab === 'velocity') val = Math.abs(p.velocity);
            else val = Math.abs(p.acceleration);
            if (val > maxValue) maxValue = val;
        });

        const baseRadius = 0.3 * maxRadius;

        camCtx.beginPath();
        camProfileData.forEach((p, i) => {
            const angle = p.angle * Math.PI / 180 - Math.PI / 2;
            let value;
            if (currentTab === 'profile') {
                value = baseRadius + (p.lift / maxValue) * maxRadius * 0.7;
            } else if (currentTab === 'velocity') {
                value = baseRadius + (Math.abs(p.velocity) / maxValue) * maxRadius * 0.7;
            } else {
                value = baseRadius + (Math.abs(p.acceleration) / maxValue) * maxRadius * 0.7;
            }

            const x = centerX + Math.cos(angle) * value;
            const y = centerY + Math.sin(angle) * value;

            if (i === 0) {
                camCtx.moveTo(x, y);
            } else {
                camCtx.lineTo(x, y);
            }
        });
        camCtx.closePath();

        const gradient = camCtx.createRadialGradient(centerX, centerY, 0, centerX, centerY, maxRadius);
        gradient.addColorStop(0, 'rgba(233, 69, 96, 0.1)');
        gradient.addColorStop(1, 'rgba(233, 69, 96, 0.3)');
        camCtx.fillStyle = gradient;
        camCtx.fill();

        camCtx.strokeStyle = '#e94560';
        camCtx.lineWidth = 2;
        camCtx.stroke();

        camCtx.fillStyle = '#aaa';
        camCtx.font = '12px sans-serif';
        camCtx.textAlign = 'left';
        camCtx.fillText(`最大值: ${maxValue.toFixed(4)}`, 10, 20);
    }

    let forceCtx, forceCanvas;

    function initForceChart() {
        forceCanvas = document.getElementById('force-chart');
        if (!forceCanvas) return;
        forceCtx = forceCanvas.getContext('2d');
        resizeForceChart();
        window.addEventListener('resize', resizeForceChart);
    }

    function resizeForceChart() {
        if (!forceCanvas) return;
        const rect = forceCanvas.parentElement.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        forceCanvas.width = rect.width * dpr;
        forceCanvas.height = 200 * dpr;
        forceCanvas.style.width = rect.width + 'px';
        forceCanvas.style.height = '200px';
        forceCtx.scale(dpr, dpr);
    }

    function drawForceChart() {
        if (!forceCanvas || !forceCtx) {
            initForceChart();
            if (!forceCanvas || !forceCtx) return;
        }

        const width = forceCanvas.width / (window.devicePixelRatio || 1);
        const height = forceCanvas.height / (window.devicePixelRatio || 1);

        forceCtx.clearRect(0, 0, width, height);

        forceCtx.fillStyle = 'rgba(0, 0, 0, 0.2)';
        forceCtx.fillRect(0, 0, width, height);

        forceCtx.strokeStyle = 'rgba(255, 255, 255, 0.05)';
        forceCtx.lineWidth = 1;
        for (let i = 0; i <= 5; i++) {
            const y = height * i / 5;
            forceCtx.beginPath();
            forceCtx.moveTo(0, y);
            forceCtx.lineTo(width, y);
            forceCtx.stroke();
        }

        if (forceHistory.length < 2) return;

        const maxForce = Math.max(...forceHistory.map(d => d.force), 100);
        const barWidth = width / maxForceHistory - 2;

        forceHistory.forEach((d, i) => {
            const x = i * (width / maxForceHistory);
            const barHeight = (d.force / maxForce) * (height - 30);
            const y = height - barHeight - 10;

            const gradient = forceCtx.createLinearGradient(x, y, x, height - 10);
            if (d.force > 800) {
                gradient.addColorStop(0, '#ef4444');
                gradient.addColorStop(1, '#7f1d1d');
            } else if (d.force > 400) {
                gradient.addColorStop(0, '#f59e0b');
                gradient.addColorStop(1, '#92400e');
            } else {
                gradient.addColorStop(0, '#4ade80');
                gradient.addColorStop(1, '#166534');
            }

            forceCtx.fillStyle = gradient;
            forceCtx.fillRect(x + 1, y, barWidth, barHeight);
        });

        forceCtx.fillStyle = '#888';
        forceCtx.font = '11px sans-serif';
        forceCtx.textAlign = 'left';
        forceCtx.fillText(`${maxForce.toFixed(0)} N`, 5, 15);
        forceCtx.fillText('0 N', 5, height - 5);
    }

    async function loadCamProfile() {
        try {
            const response = await fetch(`${apiBaseUrl}/cam-profile/${currentDeviceId}`);
            const data = await response.json();
            if (data.success && data.data) {
                camProfileData = data.data;
                drawCamProfile();
            }
        } catch (e) {
            console.error('Failed to load cam profile:', e);
            camProfileData = generateMockCamProfile();
            drawCamProfile();
        }
    }

    function generateMockCamProfile() {
        const points = [];
        const lift = 0.12;
        for (let i = 0; i < 360; i++) {
            const angle = i;
            const angleRad = angle * Math.PI / 180;
            let liftVal, velocity, acceleration;

            if (angleRad < Math.PI) {
                const t = angleRad / Math.PI;
                liftVal = lift * (1 - Math.cos(Math.PI * t)) / 2;
                velocity = lift * Math.PI * Math.sin(Math.PI * t) / 2;
                acceleration = lift * Math.PI * Math.PI * Math.cos(Math.PI * t) / 2;
            } else {
                const t = (angleRad - Math.PI) / Math.PI;
                liftVal = lift * (1 + Math.cos(Math.PI * t)) / 2;
                velocity = -lift * Math.PI * Math.sin(Math.PI * t) / 2;
                acceleration = -lift * Math.PI * Math.PI * Math.cos(Math.PI * t) / 2;
            }

            points.push({
                angle,
                radius: 0.15 + liftVal,
                lift: liftVal,
                velocity,
                acceleration
            });
        }
        return points;
    }

    function addAlert(alert) {
        const list = document.getElementById('alerts-list');
        if (!list) return;

        if (list.querySelector('.text-center, [style*="text-align: center"]')) {
            list.innerHTML = '';
        }

        const item = document.createElement('div');
        item.className = `alert-item ${alert.alert_level || 'warning'}`;

        const time = new Date(alert.timestamp || Date.now()).toLocaleTimeString();

        item.innerHTML = `
            <div class="alert-header">
                <span class="alert-type">${alert.alert_type || '未知告警'}</span>
                <span class="alert-time">${time}</span>
            </div>
            <div class="alert-msg">${alert.alert_message || ''}</div>
        `;

        list.insertBefore(item, list.firstChild);

        while (list.children.length > 20) {
            list.removeChild(list.lastChild);
        }

        if (alert.alert_level === 'critical' || alert.alert_level === 'Critical') {
            document.getElementById('vibration-card')?.classList.add('critical');
        }
    }

    function connectWebSocket() {
        const ws = new WebSocket('ws://localhost:8080/ws/alerts');
        ws.onopen = () => {
            console.log('WebSocket connected');
            updateConnectionStatus('connected');
        };
        ws.onmessage = (event) => {
            try {
                const alert = JSON.parse(event.data);
                addAlert(alert);
                retryCount = 0;
            } catch (e) { console.error(e); }
        };
        ws.onclose = () => {
            console.log('WebSocket disconnected');
            updateConnectionStatus('disconnected');
            retryCount = (retryCount || 0) + 1;
            const delay = Math.min(3000 * Math.pow(1.5, retryCount - 1), 30000);
            setTimeout(connectWebSocket, delay);
        };
        ws.onerror = () => {
            updateConnectionStatus('disconnected');
        };
    }

    function updateConnectionStatus(status) {
        const dot = document.getElementById('connection-status');
        const text = document.getElementById('connection-text');

        if (status === 'connected') {
            if (dot) dot.className = 'status-dot';
            if (text) text.textContent = '系统运行中';
        } else {
            if (dot) dot.className = 'status-dot warning';
            if (text) text.textContent = '连接断开';
        }
    }

    let isOptimizing = false;

    async function handleOptimize() {
        if (isOptimizing) return;

        isOptimizing = true;
        const btn = document.getElementById('btn-optimize');
        btn.textContent = '优化中...';
        btn.disabled = true;

        const request = {
            device_id: currentDeviceId,
            target_efficiency: parseFloat(document.getElementById('opt-target-eff').value),
            grain_type: document.getElementById('grain-type').value,
            constraints: {
                max_cam_radius: parseFloat(document.getElementById('opt-base-radius').value) * 1.5,
                min_cam_radius: 0.05,
                max_lift: parseFloat(document.getElementById('opt-lift').value) * 1.5,
                max_pressure_angle: parseFloat(document.getElementById('opt-pressure-angle').value) * Math.PI / 180
            }
        };

        try {
            const response = await fetch(`${apiBaseUrl}/optimize`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(request)
            });

            const data = await response.json();

            if (data.success && data.data) {
                const result = data.data;
                document.getElementById('opt-results').style.display = 'grid';
                document.getElementById('opt-efficiency').textContent = ((result.overall_efficiency || result.actual_efficiency || 0) * 100).toFixed(1) + '%';
                document.getElementById('opt-profile-type').textContent = getProfileTypeName(result.cam_profile_type);
                document.getElementById('opt-base-result').textContent = (result.base_radius || result.cam_base_radius || 0).toFixed(3) + 'm';
                document.getElementById('opt-lift-result').textContent = (result.lift || result.cam_lift || 0).toFixed(3) + 'm';

                if (result.cam_profile || result.cam_profile_points) {
                    camProfileData = result.cam_profile || result.cam_profile_points;
                    drawCamProfile();
                }
            }
        } catch (e) {
            console.error('Optimization failed:', e);
            const mockResult = mockOptimization();
            document.getElementById('opt-results').style.display = 'grid';
            document.getElementById('opt-efficiency').textContent = (mockResult.efficiency * 100).toFixed(1) + '%';
            document.getElementById('opt-profile-type').textContent = '摆线凸轮';
            document.getElementById('opt-base-result').textContent = mockResult.baseRadius.toFixed(3) + 'm';
            document.getElementById('opt-lift-result').textContent = mockResult.lift.toFixed(3) + 'm';
        }

        isOptimizing = false;
        btn.textContent = '开始优化';
        btn.disabled = false;
    }

    function getProfileTypeName(type) {
        const names = {
            'cycloidal': '摆线凸轮',
            'harmonic': '简谐凸轮',
            'trapezoidal': '梯形加速度凸轮',
            'polynomial': '多项式凸轮'
        };
        return names[type] || type;
    }

    function mockOptimization() {
        return {
            efficiency: 0.87,
            baseRadius: 0.18,
            lift: 0.14
        };
    }

    function startDataPolling() {
        setInterval(() => {
            fetchLatestData();
        }, 1000);
    }

    async function fetchLatestData() {
        try {
            const response = await fetch(`${apiBaseUrl}/sensor-data?device_id=${currentDeviceId}&limit=1`);
            const data = await response.json();
            if (data.success && data.data && data.data.length > 0) {
                updateSensorDisplay(data.data[0]);
            }
        } catch (e) {
        }
    }

    function setChartUpdateCallback(cb) {
        chartUpdateCallback = cb;
    }

    function triggerChartUpdate() {
        if (chartUpdateCallback) chartUpdateCallback();
    }

    global.initCamPanel = initCamPanel;
    global.initCamCanvas = initCamCanvas;
    global.initForceChart = initForceChart;
    global.drawCamProfile = drawCamProfile;
    global.drawForceChart = drawForceChart;
    global.loadCamProfile = loadCamProfile;
    global.updateSensorDisplay = updateSensorDisplay;
    global.updateDynamicsDisplay = updateDynamicsDisplay;
    global.addForceValue = addForceValue;
    global.handleOptimize = handleOptimize;
    global.connectWebSocket = connectWebSocket;
    global.setChartUpdateCallback = setChartUpdateCallback;
    global.triggerChartUpdate = triggerChartUpdate;
    global.setApiBaseUrl = (url) => { apiBaseUrl = url; };

    Object.defineProperty(global, 'forceHistory', { get: () => forceHistory });
    Object.defineProperty(global, 'currentTab', { get: () => currentTab });
    Object.defineProperty(global, 'camProfileData', { get: () => camProfileData });
    Object.defineProperty(global, 'currentDeviceId', { get: () => currentDeviceId });
    Object.defineProperty(global, 'deviceParams', { get: () => deviceParams });

})(typeof window !== 'undefined' ? window : this);
