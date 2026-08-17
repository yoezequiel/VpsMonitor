# Self-Hosted Infrastructure Monitor

## 1. Descripción

**Self-Hosted Infrastructure Monitor** es una plataforma de monitorización de infraestructura diseñada para desarrolladores, administradores de sistemas y pequeñas organizaciones que necesitan supervisar servidores, ordenadores, VPS, máquinas virtuales, dispositivos de red y contenedores desde un único punto de control.

El sistema está diseñado bajo una filosofía **self-hosted y local-first**: no depende de un servicio cloud externo para funcionar y permite que cualquier máquina pueda actuar como:

- **Monitor**
- **Agent**
- **Monitor + Agent**

Una instalación puede monitorizarse a sí misma y, al mismo tiempo, supervisar otras máquinas.

El objetivo es ofrecer una alternativa sencilla de desplegar y operar frente a plataformas de observabilidad más complejas, evitando que el usuario tenga que montar una infraestructura completa solamente para poder monitorizar sus servidores.

---

# 2. Objetivos

## Objetivo principal

Crear una herramienta de monitorización de infraestructura que permita instalar un monitor en una máquina personal o servidor y comenzar a supervisar otros dispositivos de forma rápida, segura y centralizada.

## Objetivos secundarios

- Monitorizar múltiples servidores desde una única interfaz.
- Permitir que cada instalación sea independiente.
- Permitir monitorización local sin conexión a servicios externos.
- Monitorizar el propio servidor donde se ejecuta el monitor.
- Detectar servidores y servicios fuera de línea.
- Supervisar recursos del sistema.
- Supervisar contenedores Docker.
- Realizar health checks sobre aplicaciones.
- Generar alertas.
- Registrar incidentes.
- Mantener histórico de métricas.
- Facilitar la instalación del Agent.
- Permitir instalaciones completamente automatizadas.
- Mantener una arquitectura modular y extensible.
- Priorizar seguridad y privacidad.

---

# 3. Principios del proyecto

## Self-hosted

El usuario controla dónde se ejecuta el sistema.

No es obligatorio utilizar un servicio externo.

## Local-first

La aplicación debe funcionar correctamente en una red local incluso sin acceso a Internet.

## Simple

La instalación y configuración deben requerir el mínimo conocimiento posible.

## Secure by default

Las comunicaciones, credenciales y permisos deben diseñarse bajo el principio de mínimo privilegio.

## Observable

El sistema debe poder explicar no solamente que algo está fallando, sino cuándo comenzó, cuánto duró y qué ocurrió alrededor del incidente.

## Modular

Las funcionalidades deben poder ampliarse sin modificar el núcleo del sistema.

---

# 4. Arquitectura conceptual

El sistema se divide principalmente en dos componentes:

```text
                    ┌────────────────────────┐
                    │        MONITOR         │
                    │                        │
                    │ Dashboard              │
                    │ API                    │
                    │ Database               │
                    │ Alert Manager          │
                    │ Scheduler              │
                    └───────────┬────────────┘
                                │
                 ┌──────────────┼──────────────┐
                 │              │              │
                 ▼              ▼              ▼
             SERVER 01      SERVER 02      SERVER 03
               Agent          Agent          Agent
```

Sin embargo, cualquier instalación puede ejecutar ambos componentes:

```text
┌───────────────────────────────┐
│         MONITOR NODE          │
│                               │
│ Dashboard                     │
│ API                           │
│ Database                      │
│ Agent                         │
│                               │
│ Self-monitoring: ENABLED      │
└───────────────────────────────┘
```

---

# 5. Modos de funcionamiento

## 5.1 Monitor

El nodo actúa como servidor central.

Responsabilidades:

- Recibir métricas.
- Almacenar información.
- Gestionar servidores.
- Ejecutar health checks.
- Procesar alertas.
- Registrar incidentes.
- Mostrar dashboards.
- Gestionar usuarios y permisos.

---

## 5.2 Agent

El nodo actúa como agente de monitorización.

Responsabilidades:

- Recoger métricas del sistema.
- Recoger información de Docker.
- Ejecutar comprobaciones locales.
- Enviar métricas al Monitor.
- Mantener comunicación mediante heartbeat.
- Informar de cambios relevantes.

El Agent no necesita exponer un puerto público para funcionar.

La comunicación debe ser preferentemente:

```text
Agent ─────── HTTPS ───────> Monitor
```

---

## 5.3 Monitor + Agent

Es el modo recomendado para instalaciones normales.

El mismo equipo:

1. Monitoriza su propio estado.
2. Recibe métricas de otros servidores.
3. Monitoriza servicios externos.
4. Genera alertas.

```text
              ┌───────────────────┐
              │   MONITOR + AGENT │
              │                   │
              │   Self monitoring │
              └─────────┬─────────┘
                        │
              ┌─────────┼─────────┐
              ▼         ▼         ▼
            VPS 01    VPS 02    NAS
```

---

# 6. Instalación

La aplicación debe distribuirse como software instalable.

Se buscará que la experiencia sea:

```bash
monitor install
```

El instalador permitirá seleccionar:

```text
Installation mode

> Monitor + Agent
  Monitor
  Agent
```

También deberá existir la posibilidad de instalar únicamente el Agent.

---

# 7. Aplicación de escritorio

La aplicación tendrá una interfaz gráfica para:

- Visualizar servidores.
- Administrar agentes.
- Consultar métricas.
- Configurar alertas.
- Consultar incidentes.
- Consultar logs.
- Gestionar usuarios.
- Configurar el sistema.

La aplicación deberá funcionar inicialmente en:

- Linux
- Windows
- macOS

La tecnología de escritorio podrá basarse en **Tauri**, utilizando un frontend web y un backend nativo.

---

# 8. CLI

El proyecto deberá disponer de una interfaz de línea de comandos.

Ejemplos:

```bash
monitor status
```

```bash
monitor server list
```

```bash
monitor server add
```

```bash
monitor agent status
```

```bash
monitor alerts
```

```bash
monitor logs
```

```bash
monitor incident list
```

La CLI será especialmente importante para servidores sin interfaz gráfica.

---

# 9. Registro de servidores

Desde el Monitor se podrán añadir servidores.

Cada servidor tendrá información como:

```text
Name
Hostname
IP
Operating System
Architecture
Agent version
Monitor status
Last heartbeat
Tags
```

Ejemplo:

```text
VPS-01

Ubuntu 24.04
x86_64
Agent 1.0.0

Status:
ONLINE

Last heartbeat:
12 seconds ago
```

---

# 10. Instalación remota del Agent

Una de las funcionalidades principales será permitir instalar el Agent remotamente.

El usuario podrá proporcionar:

```text
Hostname / IP
SSH user
SSH port
Authentication
```

El Monitor realizará:

```text
SSH connection
       ↓
Upload Agent
       ↓
Install service
       ↓
Generate credentials
       ↓
Configure Agent
       ↓
Start Agent
       ↓
Health check
```

Resultado:

```text
Agent installed successfully.

Server:
VPS-01

Status:
ONLINE
```

---

# 11. Métricas del sistema

El Agent recogerá información sobre:

## CPU

- Porcentaje de utilización.
- Número de cores.
- Load average.
- Uso por core.
- Procesos principales.

## Memoria

- Memoria total.
- Memoria utilizada.
- Memoria disponible.
- Swap.
- Porcentaje de utilización.

## Almacenamiento

- Discos.
- Particiones.
- Capacidad.
- Espacio utilizado.
- Espacio disponible.
- Porcentaje utilizado.

## Red

- Bytes recibidos.
- Bytes enviados.
- Paquetes.
- Interfaces.
- Errores.
- Velocidad de transferencia.

## Sistema

- Sistema operativo.
- Kernel.
- Arquitectura.
- Hostname.
- Uptime.
- Número de procesos.

---

# 12. Dashboard

El dashboard principal mostrará el estado global de la infraestructura.

Ejemplo:

```text
INFRASTRUCTURE

🟢 8 Online
🔴 1 Offline
⚠ 2 Warning

CPU
Average: 42%

MEMORY
Average: 58%

DISK
Average: 61%

ACTIVE INCIDENTS
2
```

---

# 13. Dashboard individual

Cada servidor tendrá una página detallada.

```text
VPS-01

🟢 ONLINE

CPU             32%
Memory          54%
Disk            61%
Load            1.24
Uptime          43d 17h
```

También se mostrarán gráficas históricas.

---

# 14. Histórico de métricas

El sistema almacenará métricas para poder consultar períodos anteriores.

Períodos:

- Última hora.
- 6 horas.
- 24 horas.
- 7 días.
- 30 días.
- Personalizado.

Se podrán consultar:

- CPU.
- RAM.
- Disk.
- Network.
- Load.
- Docker.
- Health checks.

---

# 15. Docker Monitoring

El Agent podrá integrarse con Docker.

Se mostrará:

```text
DOCKER

nginx       🟢 Running
postgres    🟢 Running
redis       🟢 Running
my-api      🔴 Stopped
```

Para cada contenedor:

- Estado.
- CPU.
- Memoria.
- Red.
- Número de reinicios.
- Fecha de inicio.
- Imagen.
- Versión.
- Logs recientes.

---

# 16. Health Checks

El Monitor permitirá comprobar servicios externos.

Ejemplo:

```text
https://api.example.com/health
```

Configuración:

```text
Interval: 30 seconds
Timeout: 5 seconds
Expected status: 200
```

Se registrará:

- Status code.
- Latencia.
- Disponibilidad.
- Tiempo de respuesta.
- Último fallo.

---

# 17. Tipos de Health Check

Se podrán implementar distintos tipos:

### HTTP

```text
GET /health
```

### HTTPS

Con validación TLS.

### TCP

```text
host:5432
```

### Ping

Para comprobar conectividad.

### DNS

Para verificar resolución.

### Custom

Permitir extensiones para comprobaciones específicas.

---

# 18. Heartbeat

Cada Agent enviará periódicamente un heartbeat.

Ejemplo:

```text
Agent → Monitor

HEARTBEAT
Server: VPS-01
Timestamp: ...
```

Si el Monitor deja de recibir heartbeats:

```text
SERVER OFFLINE
```

El sistema deberá distinguir entre:

- Agent offline.
- Servidor offline.
- Problema de red.
- Monitor inaccesible.

---

# 19. Alertas

El usuario podrá crear reglas.

Ejemplos:

```text
CPU > 90% for 5 minutes
```

```text
RAM > 90%
```

```text
Disk > 85%
```

```text
Server offline > 1 minute
```

```text
Container stopped
```

```text
HTTP health check failed
```

```text
Latency > 500ms
```

---

# 20. Canales de notificación

El sistema deberá soportar diferentes canales.

Inicialmente:

- Email.
- Webhook.
- Discord.
- Telegram.

Posteriormente:

- Slack.
- Otros servicios mediante webhooks.
- Integraciones personalizadas.

---

# 21. Incidentes

Las alertas relacionadas podrán agruparse automáticamente en incidentes.

Ejemplo:

```text
INCIDENT #142

18:31 CPU HIGH
18:32 CPU HIGH
18:34 API DOWN
18:36 SERVER RESTARTED
18:38 API RECOVERED
```

El sistema calculará:

```text
Duration:
7 minutes

Affected:
VPS-01

Services:
API

Status:
RESOLVED
```

---

# 22. Timeline de incidentes

Cada incidente tendrá una línea temporal.

```text
18:31 ─ CPU exceeded 90%
18:32 ─ CPU exceeded 90%
18:34 ─ API unavailable
18:36 ─ Server restarted
18:38 ─ API recovered
```

Esto permitirá investigar problemas de infraestructura sin tener que revisar múltiples sistemas manualmente.

---

# 23. Deployment Tracking

El sistema podrá registrar deployments.

Ejemplo:

```text
DEPLOYMENT

Application:
my-api

Version:
v1.8.2

Started:
17:32

Completed:
17:34

Status:
SUCCESS
```

El sistema podrá relacionar deployments con incidentes.

Ejemplo:

```text
Deployment v1.8.2
       ↓
3 minutes
       ↓
API DOWN
```

El sistema mostrará:

```text
Possible correlation:
Incident started 3 minutes after deployment.
```

No se asumirá automáticamente que el deployment fue la causa.

---

# 24. Logs

El sistema podrá consultar logs de:

- Agent.
- Monitor.
- Docker.
- Servicios monitorizados.

Ejemplo:

```text
17:42:31 INFO Request received
17:42:31 INFO User authenticated
17:42:32 INFO Database query
17:42:32 ERROR Connection timeout
```

Se podrán filtrar por:

- Fecha.
- Nivel.
- Servidor.
- Servicio.
- Contenedor.
- Texto.

---

# 25. Tags

Los servidores podrán organizarse mediante tags.

Ejemplo:

```text
production
database
docker
personal
development
backup
```

Después:

```text
Show:
production + docker
```

---

# 26. Grupos

También podrán existir grupos.

```text
PRODUCTION

VPS-01
VPS-02
VPS-03
```

```text
DEVELOPMENT

PC-01
VPS-DEV
RASPBERRY-01
```

Esto permitirá gestionar infraestructuras grandes sin perder organización.

---

# 27. Seguridad

La seguridad será un requisito fundamental.

El sistema deberá implementar:

- Comunicación cifrada.
- Tokens de autenticación.
- Rotación de tokens.
- API keys.
- Rate limiting.
- Expiración de credenciales.
- Revocación de Agents.
- Permisos por usuario.
- RBAC.
- Auditoría.
- Mínimo privilegio.

El Agent no deberá requerir puertos públicos abiertos siempre que sea posible.

---

# 28. RBAC

Se podrán crear usuarios con diferentes permisos.

Ejemplo:

### Administrator

Acceso completo.

### Operator

Puede:

- Ver servidores.
- Consultar métricas.
- Consultar incidentes.
- Gestionar alertas.

### Viewer

Puede:

- Ver dashboards.
- Consultar métricas.

No puede modificar la infraestructura.

---

# 29. Auditoría

El sistema registrará acciones administrativas.

Ejemplo:

```text
AUDIT LOG

17:32
User: admin

Action:
Agent revoked

Server:
VPS-03
```

También:

- Login.
- Logout.
- Creación de usuarios.
- Cambio de permisos.
- Creación de alertas.
- Eliminación de servidores.
- Instalación/revocación de Agents.

---

# 30. Monitorización distribuida

Una instalación podrá monitorizar otra instalación.

Ejemplo:

```text
MONITOR A
    │
    └──── monitors ────> MONITOR B
                              │
                              ├── SERVER 01
                              ├── SERVER 02
                              └── SERVER 03
```

Esto permitirá construir infraestructuras distribuidas.

También permitirá detectar si un Monitor completo deja de responder.

---

# 31. Self Monitoring

Todo Monitor + Agent podrá monitorizarse a sí mismo.

El sistema deberá comprobar:

- CPU.
- RAM.
- Disk.
- Network.
- Procesos.
- Database.
- API.
- Dashboard.
- Agent.
- Scheduler.

Esto permitirá detectar problemas internos de la propia plataforma.

---

# 32. Base de datos

La primera versión utilizará una base de datos local.

Para instalaciones pequeñas:

```text
SQLite
```

Para instalaciones mayores podrá soportarse:

```text
PostgreSQL
```

La capa de persistencia deberá abstraerse para evitar acoplamiento excesivo.

---

# 33. API

El Monitor expondrá una API.

Ejemplos conceptuales:

```text
GET /api/servers
GET /api/servers/:id
GET /api/servers/:id/metrics
GET /api/servers/:id/docker
GET /api/incidents
GET /api/alerts
```

Los Agents utilizarán endpoints específicos para:

```text
POST /api/agent/heartbeat
POST /api/agent/metrics
POST /api/agent/events
```

---

# 34. Comunicación en tiempo real

Para información que requiera actualización inmediata se utilizará una conexión persistente.

Posibles tecnologías:

- WebSockets.
- Server-Sent Events.

Ejemplo:

```text
Agent
  ↓
Monitor
  ↓
WebSocket
  ↓
Dashboard
```

Cuando un servidor pase de:

```text
🟢 ONLINE
```

a:

```text
🔴 OFFLINE
```

el dashboard deberá actualizarse sin necesidad de recargar la página.

---

# 35. Arquitectura tecnológica inicial

Una posible implementación:

```text
Desktop
    Tauri
       │
       └── Frontend
            React / Svelte

Backend
    Go

Agent
    Go

Database
    SQLite
    PostgreSQL (future)

Realtime
    WebSocket

Container monitoring
    Docker API

Service management
    systemd

Deployment
    GitHub Actions
```

La elección definitiva de tecnologías podrá modificarse durante el desarrollo si existe una razón técnica.

---

# 36. Distribución

El proyecto deberá generar paquetes para:

- Linux
- Windows
- macOS

Linux deberá soportar inicialmente:

- x86_64
- ARM64

También se generará un binario independiente del Agent.

---

# 37. Actualizaciones

El sistema deberá poder comprobar nuevas versiones.

El usuario podrá actualizar:

```text
Current version:
1.2.0

New version:
1.3.0

[ Update ]
```

Los Agents también deberán poder actualizarse de forma controlada.

---

# 38. Backup

El Monitor deberá permitir realizar backup de:

- Configuración.
- Usuarios.
- Alertas.
- Servidores.
- Históricos.
- Incidentes.

También deberá permitir restaurar una instalación.

---

# 39. Import / Export

Se podrán exportar configuraciones.

Ejemplo:

```text
monitor-config.yaml
```

Esto permitirá migrar una instalación.

---

# 40. Plugins

La arquitectura deberá permitir futuras extensiones.

Ejemplos:

```text
plugins/
├── nginx
├── postgres
├── mysql
├── redis
├── kubernetes
└── custom-http
```

Un plugin podrá proporcionar:

- Métricas.
- Health checks.
- Alertas.
- Información específica.
- Dashboards especializados.

---

# 41. MVP

La primera versión no intentará implementar todo.

El MVP deberá incluir:

### Core

- [ ] Monitor.
- [ ] Agent.
- [ ] Monitor + Agent.
- [ ] CLI.
- [ ] Instalación local.
- [ ] Auto-identificación del nodo.

### Métricas

- [ ] CPU.
- [ ] RAM.
- [ ] Disk.
- [ ] Network.
- [ ] Load.
- [ ] Uptime.

### Dashboard

- [ ] Lista de servidores.
- [ ] Estado online/offline.
- [ ] Dashboard individual.
- [ ] Gráficas básicas.
- [ ] Actualización en tiempo real.

### Comunicación

- [ ] Registro del Agent.
- [ ] Heartbeat.
- [ ] Autenticación.
- [ ] TLS.

### Persistencia

- [ ] SQLite.
- [ ] Histórico básico.

---

# 42. V2

Después del MVP:

- [ ] Docker monitoring.
- [ ] Health checks.
- [ ] Alertas.
- [ ] Email.
- [ ] Webhooks.
- [ ] Incidentes.
- [ ] Timeline.
- [ ] Tags.
- [ ] Grupos.
- [ ] Logs.

---

# 43. V3

Funciones avanzadas:

- [ ] Instalación remota por SSH.
- [ ] Deployment tracking.
- [ ] Correlación deployment/incidente.
- [ ] RBAC.
- [ ] Auditoría.
- [ ] Discord.
- [ ] Telegram.
- [ ] Monitorización distribuida.
- [ ] Monitorización de otros Monitors.
- [ ] Plugins.
- [ ] Backup/restore.
- [ ] Actualizaciones automáticas.

---

# 44. V4

Funciones de plataforma:

- [ ] PostgreSQL.
- [ ] Alta disponibilidad.
- [ ] Clustering.
- [ ] Multi-monitor.
- [ ] Discovery automático.
- [ ] Kubernetes.
- [ ] Integraciones avanzadas.
- [ ] API pública.
- [ ] SDK.
- [ ] Marketplace de plugins.

---

# 45. Objetivo final

El producto final debe permitir que un usuario pueda pasar de:

```text
"Tengo cinco servidores y no sé qué está pasando"
```

a:

```text
$ monitor install

Monitor + Agent installed.

5 servers detected.

Infrastructure:
🟢 4 Online
🔴 1 Offline

Active incidents:
1
```

sin depender de un servicio cloud externo.

---

# 46. Filosofía de producto

El producto no pretende competir inicialmente con plataformas empresariales como Datadog, New Relic, Grafana o similares.

Su propuesta es diferente:

> **Simple infrastructure monitoring that you own.**

La herramienta debe priorizar:

1. Instalación sencilla.
2. Bajo consumo de recursos.
3. Privacidad.
4. Self-hosting.
5. Administración simple.
6. Buen UX.
7. Automatización.
8. Extensibilidad.

---

# 47. Caso de uso principal

Un desarrollador tiene:

```text
Laptop
VPS Production
VPS Development
Home Server
NAS
Raspberry Pi
```

Instala:

```text
Monitor + Agent
```

en su laptop o en uno de los servidores.

Después instala Agents en los demás dispositivos.

Resultado:

```text
MY INFRASTRUCTURE

🟢 Laptop
🟢 VPS Production
🟢 VPS Development
🟢 Home Server
🟢 NAS
🔴 Raspberry Pi

CPU       34%
RAM       51%
DISK      63%

ACTIVE INCIDENTS
1

ALERTS
2
```

Todo se gestiona desde una única aplicación.

---

# 48. Diferenciación

La diferenciación principal será:

### No Cloud Required

El sistema funciona sin una plataforma externa.

### Install Anywhere

Cualquier máquina puede convertirse en Monitor.

### Monitor Yourself

El Monitor puede monitorizarse a sí mismo.

### Monitor Monitors

Un Monitor puede supervisar otro Monitor.

### Simple Deployment

Instalación rápida y automatizada.

### Developer First

CLI, API, Docker, SSH, Webhooks y configuración reproducible.

### Privacy First

Los datos permanecen bajo control del propietario.

---

# 49. Nombre provisional

Nombre de trabajo:

**Sentinel**

Alternativas:

- Watchtower
- Overseer
- Monolith
- Vigil
- Sentinel Monitor
- NodeWatch
- InfraWatch
- ServerSentinel

El nombre definitivo se decidirá posteriormente.

---

# 50. Resultado esperado

El resultado será una aplicación de infraestructura profesional capaz de:

```text
INSTALL
   ↓
CONFIGURE
   ↓
DISCOVER
   ↓
MONITOR
   ↓
DETECT
   ↓
ALERT
   ↓
INVESTIGATE
   ↓
RECOVER
```

El proyecto deberá ser suficientemente sencillo para que un desarrollador pueda instalarlo en su ordenador en pocos minutos, pero suficientemente robusto para utilizarlo posteriormente en una infraestructura real.

---

# 51. Filosofía de desarrollo

El proyecto se desarrollará de forma incremental.

No se intentará construir todas las funcionalidades desde el principio.

Cada versión debe producir un software funcional que pueda utilizarse en infraestructura real.

La prioridad será:

**MVP funcional → estabilidad → observabilidad → automatización → seguridad → extensibilidad.**

El objetivo no es construir un dashboard espectacular.

El objetivo es construir una herramienta que, cuando un servidor falle a las 3 de la mañana, sea capaz de decir:

> **"Algo ha ocurrido. Este servidor ha dejado de responder. Ocurrió a las 03:17. Este fue el último estado conocido."**

Y que el usuario pueda actuar sobre esa información inmediatamente.
