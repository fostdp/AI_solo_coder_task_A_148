FROM python:3.12-slim

WORKDIR /app

COPY sensor_simulator.py .
RUN pip install --no-cache-dir paho-mqtt

ENV MQTT_BROKER=localhost
ENV MQTT_PORT=1883
ENV CAM_PROFILE=harmonic
ENV GRAIN_TYPE=rice
ENV INTERVAL=1.0

CMD ["python", "sensor_simulator.py", \
     "--broker", "${MQTT_BROKER}", \
     "--port", "${MQTT_PORT}", \
     "--cam-profile", "${CAM_PROFILE}", \
     "--grain", "${GRAIN_TYPE}", \
     "--interval", "${INTERVAL}"]
