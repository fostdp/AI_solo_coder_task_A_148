(function (global) {
    'use strict';

    let scene, camera, renderer, controls;
    let waterWheel, cam, duitou, frame;
    let camUniforms;
    let animationSpeed = 1;
    let lastChartUpdate = 0;
    let resizeHandler;

    const camVertexShader = `
        uniform float uBaseRadius;
        uniform float uLift;
        uniform float uRotation;
        uniform float uThickness;
        uniform float uTime;

        varying vec3 vNormal;
        varying vec3 vPosition;
        varying float vCamAngle;
        varying float vRadius;

        float harmonicLift(float angle, float totalLift) {
            float pi = 3.14159265359;
            float a = mod(angle, pi * 2.0);
            float t;
            if (a < pi) {
                t = a / pi;
                return totalLift * (1.0 - cos(pi * t)) / 2.0;
            } else {
                t = (a - pi) / pi;
                return totalLift * (1.0 + cos(pi * t)) / 2.0;
            }
        }

        void main() {
            float theta = atan(position.z, position.x);
            float baseR = length(vec2(position.x, position.z));

            float adjustedTheta = theta + uRotation;
            float dynamicLift = harmonicLift(adjustedTheta, uLift);
            float r = uBaseRadius + dynamicLift;

            float scale = r / baseR;
            vec3 newPos = position;
            newPos.x *= scale;
            newPos.z *= scale;

            vCamAngle = adjustedTheta;
            vRadius = r;
            vNormal = normalize(normalMatrix * normal);
            vPosition = newPos;

            gl_Position = projectionMatrix * modelViewMatrix * vec4(newPos, 1.0);
        }
    `;

    const camFragmentShader = `
        uniform vec3 uColor;
        uniform float uMetalness;
        uniform float uRoughness;
        uniform float uLift;

        varying vec3 vNormal;
        varying vec3 vPosition;
        varying float vCamAngle;
        varying float vRadius;

        void main() {
            vec3 lightDir = normalize(vec3(1.0, 1.0, 1.0));
            float diff = max(dot(normalize(vNormal), lightDir), 0.0);

            vec3 viewDir = normalize(cameraPosition - vPosition);
            vec3 halfDir = normalize(lightDir + viewDir);
            float spec = pow(max(dot(normalize(vNormal), halfDir), 0.0), 32.0) * uMetalness;

            float rim = 1.0 - max(dot(viewDir, normalize(vNormal)), 0.0);
            rim = pow(rim, 3.0) * 0.3;

            vec3 ambient = uColor * 0.3;
            vec3 diffuse = uColor * diff * (1.0 - uRoughness * 0.5);
            vec3 specular = vec3(1.0, 0.95, 0.85) * spec;
            vec3 rimLight = vec3(1.0, 0.7, 0.4) * rim;

            vec3 finalColor = ambient + diffuse + specular + rimLight;

            gl_FragColor = vec4(finalColor, 1.0);
        }
    `;

    function initThreeScene(containerId) {
        return new Promise((resolve) => {
            const container = document.getElementById(containerId);
            const canvas = container.querySelector('#canvas-3d');
            const rect = container.getBoundingClientRect();

            scene = new THREE.Scene();
            scene.background = new THREE.Color(0x0a0a1a);
            scene.fog = new THREE.Fog(0x0a0a1a, 10, 30);

            camera = new THREE.PerspectiveCamera(60, rect.width / rect.height, 0.1, 1000);
            camera.position.set(5, 4, 6);
            camera.lookAt(0, 1, 0);

            renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
            renderer.setSize(rect.width, rect.height);
            renderer.shadowMap.enabled = true;

            const ambientLight = new THREE.AmbientLight(0x404060, 0.5);
            scene.add(ambientLight);

            const directionalLight = new THREE.DirectionalLight(0xffffff, 0.8);
            directionalLight.position.set(5, 10, 5);
            directionalLight.castShadow = true;
            scene.add(directionalLight);

            const hemisphereLight = new THREE.HemisphereLight(0x87ceeb, 0x362d26, 0.4);
            scene.add(hemisphereLight);

            if (typeof THREE.OrbitControls !== 'undefined') {
                controls = new THREE.OrbitControls(camera, renderer.domElement);
                controls.enableDamping = true;
                controls.dampingFactor = 0.05;
                controls.target.set(0, 1, 0);
            }

            const groundGeometry = new THREE.PlaneGeometry(15, 15);
            const groundMaterial = new THREE.MeshStandardMaterial({
                color: 0x2d3436,
                roughness: 0.9,
                metalness: 0.1
            });
            const ground = new THREE.Mesh(groundGeometry, groundMaterial);
            ground.rotation.x = -Math.PI / 2;
            ground.receiveShadow = true;
            scene.add(ground);

            const gridHelper = new THREE.GridHelper(10, 20, 0x444, 0x222);
            scene.add(gridHelper);

            frame = buildFrame();
            scene.add(frame);

            waterWheel = buildWaterWheel();
            waterWheel.position.set(-2, 2.5, 0);
            scene.add(waterWheel);

            cam = buildCam();
            cam.position.set(0, 2.5, 0);
            scene.add(cam);

            duitou = buildDuitou();
            duitou.position.set(0, 1.8, 0);
            scene.add(duitou);

            buildGrainPit();

            resizeHandler = function () {
                const r = container.getBoundingClientRect();
                camera.aspect = r.width / r.height;
                camera.updateProjectionMatrix();
                renderer.setSize(r.width, r.height);
            };
            window.addEventListener('resize', resizeHandler);

            resolve({ scene, camera, renderer, controls });
        });
    }

    function buildFrame() {
        const group = new THREE.Group();

        const pillarGeometry = new THREE.BoxGeometry(0.3, 4, 0.3);
        const pillarMaterial = new THREE.MeshStandardMaterial({
            color: 0x8B4513,
            roughness: 0.7,
            metalness: 0.1
        });

        const positions = [
            [-1.5, 2, -0.8],
            [1.5, 2, -0.8],
            [-1.5, 2, 0.8],
            [1.5, 2, 0.8]
        ];

        positions.forEach(pos => {
            const pillar = new THREE.Mesh(pillarGeometry, pillarMaterial);
            pillar.position.set(...pos);
            pillar.castShadow = true;
            group.add(pillar);
        });

        const beamGeometry = new THREE.BoxGeometry(3.5, 0.25, 0.25);
        const beamMaterial = new THREE.MeshStandardMaterial({
            color: 0x654321,
            roughness: 0.8
        });

        const topBeam = new THREE.Mesh(beamGeometry, beamMaterial);
        topBeam.position.set(0, 3.9, 0);
        topBeam.castShadow = true;
        group.add(topBeam);

        const sideBeam1 = new THREE.Mesh(beamGeometry, beamMaterial);
        sideBeam1.position.set(0, 2.5, -0.8);
        sideBeam1.castShadow = true;
        group.add(sideBeam1);

        const sideBeam2 = new THREE.Mesh(beamGeometry, beamMaterial);
        sideBeam2.position.set(0, 2.5, 0.8);
        sideBeam2.castShadow = true;
        group.add(sideBeam2);

        return group;
    }

    function buildWaterWheel() {
        const group = new THREE.Group();

        const wheelGeometry = new THREE.CylinderGeometry(1.2, 1.2, 0.6, 16);
        const wheelMaterial = new THREE.MeshStandardMaterial({
            color: 0x654321,
            roughness: 0.8,
            metalness: 0.1
        });
        const wheel = new THREE.Mesh(wheelGeometry, wheelMaterial);
        wheel.rotation.z = Math.PI / 2;
        wheel.castShadow = true;
        group.add(wheel);

        const bladeGeometry = new THREE.BoxGeometry(0.08, 0.8, 0.5);
        const bladeMaterial = new THREE.MeshStandardMaterial({
            color: 0x8B4513,
            roughness: 0.9
        });

        for (let i = 0; i < 8; i++) {
            const blade = new THREE.Mesh(bladeGeometry, bladeMaterial);
            const angle = (i / 8) * Math.PI * 2;
            blade.position.set(
                Math.cos(angle) * 1.0,
                Math.sin(angle) * 1.0,
                0
            );
            blade.rotation.z = angle;
            blade.castShadow = true;
            group.add(blade);
        }

        const axleGeometry = new THREE.CylinderGeometry(0.1, 0.1, 0.8, 12);
        const axleMaterial = new THREE.MeshStandardMaterial({
            color: 0x444,
            metalness: 0.8,
            roughness: 0.3
        });
        const axle = new THREE.Mesh(axleGeometry, axleMaterial);
        axle.rotation.z = Math.PI / 2;
        group.add(axle);

        return group;
    }

    function buildCam() {
        const group = new THREE.Group();

        const baseRadius = 0.5;
        const lift = 0.3;
        const thickness = 0.2;

        camUniforms = {
            uBaseRadius: { value: baseRadius },
            uLift: { value: lift },
            uRotation: { value: 0.0 },
            uThickness: { value: thickness },
            uTime: { value: 0.0 },
            uColor: { value: new THREE.Color(0xcd853f) },
            uMetalness: { value: 0.6 },
            uRoughness: { value: 0.4 }
        };

        const camGeometry = new THREE.CylinderGeometry(
            baseRadius + lift * 0.5,
            baseRadius + lift * 0.5,
            thickness,
            128,
            1,
            false
        );

        const camMaterial = new THREE.ShaderMaterial({
            uniforms: camUniforms,
            vertexShader: camVertexShader,
            fragmentShader: camFragmentShader
        });

        const camMesh = new THREE.Mesh(camGeometry, camMaterial);
        camMesh.castShadow = true;
        camMesh.receiveShadow = true;
        group.add(camMesh);

        const hubGeometry = new THREE.CylinderGeometry(0.12, 0.12, thickness + 0.1, 24);
        const hubMaterial = new THREE.MeshStandardMaterial({
            color: 0x8B7355,
            metalness: 0.5,
            roughness: 0.5
        });
        const hub = new THREE.Mesh(hubGeometry, hubMaterial);
        hub.rotation.x = Math.PI / 2;
        hub.castShadow = true;
        group.add(hub);

        return group;
    }

    function buildDuitou() {
        const group = new THREE.Group();

        const headGeometry = new THREE.CylinderGeometry(0.3, 0.4, 0.5, 16);
        const headMaterial = new THREE.MeshStandardMaterial({
            color: 0x696969,
            metalness: 0.7,
            roughness: 0.3
        });
        const head = new THREE.Mesh(headGeometry, headMaterial);
        head.position.y = -0.25;
        head.castShadow = true;
        group.add(head);

        const shaftGeometry = new THREE.CylinderGeometry(0.1, 0.12, 1.5, 12);
        const shaftMaterial = new THREE.MeshStandardMaterial({
            color: 0x8B4513,
            roughness: 0.8
        });
        const shaft = new THREE.Mesh(shaftGeometry, shaftMaterial);
        shaft.position.y = 0.75;
        shaft.castShadow = true;
        group.add(shaft);

        const rollerGeometry = new THREE.CylinderGeometry(0.15, 0.15, 0.3, 12);
        const rollerMaterial = new THREE.MeshStandardMaterial({
            color: 0x444,
            metalness: 0.8,
            roughness: 0.2
        });
        const roller = new THREE.Mesh(rollerGeometry, rollerMaterial);
        roller.position.set(0, 1.5, 0);
        roller.rotation.z = Math.PI / 2;
        roller.castShadow = true;
        group.add(roller);

        return group;
    }

    function buildGrainPit() {
        const pitGeometry = new THREE.CylinderGeometry(0.5, 0.45, 0.6, 24, 1, true);
        const pitMaterial = new THREE.MeshStandardMaterial({
            color: 0x8B4513,
            roughness: 0.9,
            side: THREE.DoubleSide
        });
        const pit = new THREE.Mesh(pitGeometry, pitMaterial);
        pit.position.y = -0.3;
        pit.receiveShadow = true;
        scene.add(pit);

        const grainGeometry = new THREE.CylinderGeometry(0.45, 0.4, 0.3, 24);
        const grainMaterial = new THREE.MeshStandardMaterial({
            color: 0xf0e68c,
            roughness: 0.9
        });
        const grain = new THREE.Mesh(grainGeometry, grainMaterial);
        grain.position.y = -0.4;
        grain.name = 'grain';
        scene.add(grain);
    }

    function animateFrame({ camAngleDeg, deviceParams, forceValue, huskingRate }) {
        const angleRad = camAngleDeg * Math.PI / 180;
        const lift = (deviceParams && deviceParams.cam_lift) ? deviceParams.cam_lift * 2.5 : 0.3;
        let duitouLift;

        if (angleRad < Math.PI) {
            const t = angleRad / Math.PI;
            duitouLift = lift * (1 - Math.cos(Math.PI * t)) / 2;
        } else {
            const t = (angleRad - Math.PI) / Math.PI;
            duitouLift = lift * (1 + Math.cos(Math.PI * t)) / 2;
        }

        if (waterWheel) {
            waterWheel.rotation.z -= 0.03 * animationSpeed;
        }
        if (cam && camUniforms) {
            camUniforms.uRotation.value = -angleRad;
            camUniforms.uTime.value = performance.now() * 0.001;
        }
        if (duitou) {
            duitou.position.y = 1.8 + duitouLift;
        }

        const now = Date.now();
        let shouldUpdateCharts = false;
        if (now - lastChartUpdate > 33) {
            shouldUpdateCharts = true;
            lastChartUpdate = now;
        }

        if (controls) {
            controls.update();
        }

        if (renderer && scene && camera) {
            renderer.render(scene, camera);
        }

        let nextAngle = camAngleDeg + 1.5 * animationSpeed;
        if (nextAngle >= 360) nextAngle -= 360;
        if (nextAngle < 0) nextAngle += 360;

        return {
            nextAngle,
            duitouPosition: duitouLift,
            shouldUpdateCharts
        };
    }

    function disposeThree() {
        if (resizeHandler) {
            window.removeEventListener('resize', resizeHandler);
            resizeHandler = null;
        }
        if (controls && typeof controls.dispose === 'function') {
            controls.dispose();
        }
        if (renderer) {
            renderer.dispose();
        }
        scene = null;
        camera = null;
        renderer = null;
        controls = null;
        waterWheel = null;
        cam = null;
        duitou = null;
        frame = null;
        camUniforms = null;
    }

    function setAnimationSpeed(speed) {
        animationSpeed = speed;
    }

    function getCamUniforms() {
        return camUniforms;
    }

    global.initThreeScene = initThreeScene;
    global.buildWaterWheel = buildWaterWheel;
    global.buildFrame = buildFrame;
    global.buildCam = buildCam;
    global.buildDuitou = buildDuitou;
    global.buildGrainPit = buildGrainPit;
    global.animateFrame = animateFrame;
    global.disposeThree = disposeThree;
    global.setAnimationSpeed = setAnimationSpeed;
    global.getCamUniforms = getCamUniforms;

    Object.defineProperty(global, 'scene', { get: () => scene });
    Object.defineProperty(global, 'camera', { get: () => camera });
    Object.defineProperty(global, 'renderer', { get: () => renderer });

})(typeof window !== 'undefined' ? window : this);
