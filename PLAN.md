# Plan de Ejecución — Self-Hosted Infrastructure Monitor

> Plan técnico derivado del análisis de `Proyecto.md`. Traduce la visión de producto en arquitectura concreta, estructura de repositorio, modelo de datos, contratos de API y una hoja de ruta ejecutable fase a fase.

---

## 1. Análisis crítico del documento base

Puntos fuertes del planteamiento:

- La distinción **Monitor / Agent / Monitor+Agent** es el núcleo arquitectónico correcto: todo lo demás (dashboard, alertas) es una capa sobre esa relación cliente-servidor.
- El roadmap MVP → V2 → V3 → V4 ya está bien priorizado: primero observabilidad básica, después alertas/incidentes, después automatización/seguridad avanzada, y al final escalabilidad (Postgres, HA, clustering).
- El principio "Agent no necesita puerto público" es correcto y condiciona el diseño de comunicación: debe ser **Agent → Monitor** (push), nunca al revés, salvo instalación remota por SSH (que es un flujo distinto, iniciado por el Monitor una única vez).

Riesgos / ambigüedades que este plan resuelve con una decisión explícita (marcadas `[DECISIÓN]`) para no bloquear el desarrollo:

- El documento no especifica el protocolo exacto Agent↔Monitor (REST puro vs REST+WebSocket vs gRPC). **[DECISIÓN]**: HTTPS/REST para heartbeat y métricas (simple, fácil de depurar, funciona detrás de NAT), WebSocket solo Monitor→Dashboard para tiempo real.
- No se especifica el mecanismo de emparejamiento inicial Agent-Monitor (pairing). **[DECISIÓN]**: token de un solo uso generado por el Monitor (`monitor server add`), similar a como Tailscale/Wireguard hacen onboarding.
- "Monitorización distribuida" (Monitor supervisando Monitor) reutiliza el mismo protocolo que Agent→Monitor, tratando un Monitor remoto como un Agent especial. Esto evita construir un segundo protocolo.
- Multi-tenancy no se contempla — **una instalación = un tenant** (coherente con la filosofía self-hosted). **[DECISIÓN DE PRODUCTO]**: no habrá RBAC/roles. Cualquier usuario autenticado tiene acceso total — modelo de permisos plano, no una limitación temporal del MVP. La sección 28 del documento original (RBAC de 3 roles) queda descartada.

---

## 2. Decisiones de arquitectura

### 2.1 Stack tecnológico (actualizado a Rust — sustituye la sección 35 del documento)

El documento original proponía Go, pero el proyecto se construirá en **Rust** de punta a punta (Monitor, Agent, CLI y capa nativa de Tauri). Esto tiene una ventaja arquitectónica directa: al ser Tauri también Rust, el Monitor puede vivir como **librería embebida dentro de la app de escritorio** en lugar de como sidecar de otro lenguaje — desaparece por completo el problema de empaquetar un binario externo que mencionaba la versión anterior de este plan.

| Componente | Tecnología | Justificación |
|---|---|---|
| Monitor (backend/API) | **Rust + `axum`** (sobre `tokio`) | Async de alto rendimiento, WebSocket integrado en `axum`, bajo consumo de memoria — encaja con "self-hosted, bajo consumo" mejor incluso que Go |
| Agent | **Rust** | Mismo workspace que el Monitor → se comparten tipos vía crate `shared`; compila a binario estático (`musl` en Linux) sin runtime, footprint mínimo, ideal para Raspberry Pi/dispositivos con pocos recursos |
| Frontend web (dashboard) | **React + TypeScript + Vite** | Ecosistema maduro para gráficas (Recharts/visx) y tablas; se sirve tal cual desde el Monitor y se reutiliza dentro de Tauri sin cambios |
| Desktop | **Tauri v2** | El backend nativo de Tauri ya es Rust: el Monitor se integra como librería/comando de Tauri en vez de proceso aparte, simplificando build y distribución |
| CLI | **Rust (`clap`)** | Mismo workspace que Monitor/Agent, comparte lógica de negocio vía crate `shared`; un solo binario `monitor` con subcomandos |
| Base de datos | **SQLite** (MVP) vía **`sqlx`**, abstraída detrás de un trait `Store` | `sqlx` soporta SQLite y PostgreSQL con la misma API async, lo que reduce drásticamente el trabajo de migrar a Postgres en V4; cero dependencias externas para instalación de un usuario |
| Tiempo real | **WebSocket** vía `axum::extract::ws` | Server→Dashboard únicamente; Agent→Monitor sigue siendo REST/HTTPS |
| Docker monitoring | **`bollard`** (cliente async del Docker Engine API) desde el Agent | El Agent ya vive en el host, tiene acceso al socket local |
| Métricas de sistema | **`sysinfo`** | Crate multiplataforma (Linux/macOS/Windows) para CPU, RAM, disco, red, procesos — evita reimplementar collectors por SO |
| Instalación remota SSH | **`russh`** (o `openssh` como wrapper del binario del sistema) | Subir el binario del Agent y ejecutar comandos de instalación remota (sección 10) |
| TLS | **`rustls`** + **`rcgen`** para el certificado autofirmado del primer arranque | Sin dependencia de OpenSSL del sistema, más fácil de compilar de forma reproducible |
| Gestión de servicio | **systemd** (Linux), **launchd** (macOS), **`windows-service`** (crate, Windows) | El instalador genera la unidad correspondiente |
| CI/CD | **GitHub Actions** + `cross`/`cargo-zigbuild` | Build matrix multiplataforma (linux-x86_64, linux-arm64, darwin, windows) con compilación cruzada desde Linux |

### 2.2 Estructura de repositorio (Cargo workspace)

```text
vpsmonitor/
├── Cargo.toml                # [workspace] con todos los miembros
├── crates/
│   ├── monitor-bin/          # entrypoint del binario monitor (server+cli unificados)
│   ├── agent-bin/            # entrypoint del binario agent standalone
│   ├── monitor-core/
│   │   ├── api/               # handlers HTTP/REST (axum)
│   │   ├── ws/                 # hub de websockets para el dashboard
│   │   ├── scheduler/           # health checks, evaluación de alertas, heartbeat timeout
│   │   ├── alerting/             # reglas, evaluación, notificadores (email/webhook/discord/telegram)
│   │   ├── incidents/             # agrupación de alertas en incidentes
│   │   ├── auth/                    # tokens, sesiones (sin RBAC — acceso total si autenticado)
│   │   ├── store/                    # trait Store + implementación sqlx (sqlite; postgres en V4)
│   │   └── ssh/                        # instalación remota de Agent vía russh
│   ├── agent-core/
│   │   ├── collector/           # cpu, mem, disk, net (sysinfo) + docker (bollard)
│   │   ├── reporter/             # cliente HTTP hacia el Monitor, heartbeat loop
│   │   └── healthcheck/           # ejecutor local de checks HTTP/TCP/Ping/DNS
│   ├── shared/
│   │   ├── model/                # structs comunes (Server, Metric, Alert, Incident...), serde
│   │   └── proto/                 # contratos de API (tipos request/response compartidos)
│   └── cli/                      # subcomandos clap: status, server, agent, alerts, logs, incident
├── web/                        # frontend React+TS (dashboard), compartido por navegador y Tauri
├── desktop/                    # proyecto Tauri v2 — monitor-core como dependencia Rust directa (sin sidecar)
├── plugins/                     # (V3+) contratos de plugin + implementaciones de referencia
├── deploy/
│   ├── systemd/
│   ├── launchd/
│   └── install.sh                # script `curl | sh` estilo instalador
├── docs/
├── migrations/                   # esquema SQLite versionado (sqlx migrate)
└── .github/workflows/
```

Un solo binario (`monitor`, crate `monitor-bin`) actuará como Monitor, Agent, CLI y punto de instalación (`monitor install`) según flags/subcomando — evita distribuir binarios distintos para el caso común, aunque `agent-bin` seguirá generando también un binario **Agent standalone** minimalista (sin las dependencias del Monitor) para instalaciones donde se priorice el tamaño/footprint, como Raspberry Pi (sección 36 del documento).

### 2.3 Modelo de datos inicial (SQLite, MVP)

```text
servers(id, name, hostname, ip, os, arch, agent_version, tags_json,
        status, last_heartbeat_at, token_hash, created_at)

metrics(id, server_id, ts, cpu_pct, mem_used, mem_total, disk_used,
        disk_total, load1, load5, load15, net_rx, net_tx)
   -- índice compuesto (server_id, ts); particionado lógico por retención

users(id, username, password_hash, role, created_at)

sessions(id, user_id, token_hash, expires_at)

health_checks(id, name, type, target, interval_s, timeout_s,
              expected_status, server_id_nullable)

health_check_results(id, health_check_id, ts, status_code, latency_ms, ok)

alerts_rules(id, name, condition_json, severity, enabled)

alert_events(id, rule_id, server_id, state, started_at, resolved_at)

incidents(id, title, started_at, resolved_at, status)
incident_events(id, incident_id, ts, description, source_ref)

audit_log(id, user_id, action, target, ts, metadata_json)
```

`metrics` es la tabla de mayor volumen: desde el MVP se diseña con **retención + downsampling** en mente (ej. crudo 24h, agregados por minuto 7d, por hora 30d) aunque la implementación completa de downsampling puede diferirse a V2.

### 2.4 Contrato de comunicación Agent ↔ Monitor

```text
POST /api/agent/register        (token de pairing de un solo uso → credenciales permanentes)
POST /api/agent/heartbeat       (cada N s: server_id + timestamp)
POST /api/agent/metrics         (cada N s: snapshot de métricas)
POST /api/agent/events          (docker state changes, restarts, etc.)
```

Autenticación: cada Agent recibe, tras el registro, un **token Bearer de larga duración** (rotable) ligado a `servers.token_hash`. Todas las llamadas van sobre HTTPS. El Monitor detecta offline por ausencia de heartbeat (timeout configurable, default 60s) y distingue los 4 casos de la sección 18 comparando: último heartbeat vs. resultado de un ping/TCP directo del Monitor al Agent (si es alcanzable) para diferenciar "Agent caído" de "problema de red".

### 2.5 Seguridad (base para todas las fases)

- TLS obligatorio por defecto (certificado autofirmado generado en el primer arranque si no hay uno configurado; el usuario puede traer el suyo).
- Tokens de Agent con hash almacenado (nunca en claro), rotables desde CLI/dashboard, revocables individualmente.
- Rate limiting por IP/token en endpoints `/api/agent/*` y `/api/auth/*`.
- Sin RBAC (decisión de producto, ver sección 2 de este documento): cualquier usuario autenticado tiene acceso total. No hay roles ni niveles de permiso.
- `audit_log` se escribe desde el mismo middleware de autorización para cualquier acción mutante — así no depende de que cada feature nueva "recuerde" auditar.

---

## 3. Hoja de ruta por fases

Cada fase produce software desplegable en infraestructura real, siguiendo la filosofía de la sección 51. Estimaciones asumen 1 desarrollador a tiempo parcial/completo trabajando de forma incremental; ajustar según dedicación real.

### Fase 0 — Fundamentos (1–2 semanas)

Objetivo: repositorio listo para desarrollar sin bloqueos técnicos.

- [ ] Inicializar repo git + estructura de carpetas anterior (`Cargo.toml` de workspace + crates vacíos).
- [ ] Linting/formato: `clippy` (deny warnings en CI) + `rustfmt`, `cargo-deny` para auditar licencias/vulnerabilidades de dependencias.
- [ ] Esqueleto CLI con `clap` (`monitor --help` funcional, subcomandos como stubs).
- [ ] Definir crate `shared::model` con los structs principales (Server, Metric, User, HealthCheck, Alert, Incident) derivando `serde::{Serialize, Deserialize}`.
- [ ] Definir trait `Store` (async, vía `async-trait` o los AFIT nativos de Rust 1.75+) sin implementación aún, para no acoplar lógica a SQLite desde el día uno.
- [ ] Pipeline CI: `cargo build --workspace` + `cargo test --workspace` en Linux/macOS/Windows, `cargo clippy -- -D warnings`, compilación cruzada de prueba (`cross build --target aarch64-unknown-linux-musl`).
- [ ] Elegir y documentar convención de versionado de migraciones (`sqlx migrate add/run`).

**Criterio de salida**: `cargo build --workspace` y `cargo test --workspace` verdes en CI para las 3 plataformas, incluyendo un build cruzado a ARM64 funcional.

### Fase 1 — MVP (según sección 41)

Objetivo: un usuario instala Monitor+Agent en su máquina, añade un segundo Agent, y ve ambos en un dashboard en tiempo real.

**1a. Persistencia + Core**
- [ ] Implementar `store` SQLite + migraciones para `servers`, `metrics`, `users`, `sessions`.
- [ ] Auto-identificación del nodo (hostname, OS, arch detectados al arrancar).
- [ ] Modo de arranque: flags `--mode=monitor|agent|monitor+agent`.

**1b. Agent — recolección de métricas**
- [ ] Collector CPU/RAM/Disk/Network/Load/Uptime usando `sysinfo` como base, ya multiplataforma.
- [ ] Loop de heartbeat + envío de métricas cada N segundos (configurable) con `tokio::time::interval`.

**1c. Monitor — API + auth**
- [ ] `POST /api/agent/register`, `/heartbeat`, `/metrics` en `axum`.
- [x] Auth básica de usuario (login, sesión) — sin roles: cualquier usuario autenticado tiene acceso total (decisión de producto, no un placeholder de MVP).
- [ ] TLS con certificado autofirmado por defecto (`rustls` + `rcgen`).

**1d. Dashboard**
- [ ] Listado de servidores con estado online/offline.
- [ ] Vista individual por servidor con métricas actuales.
- [ ] Gráficas básicas (últimas N horas) usando los datos ya almacenados.
- [ ] WebSocket para actualización en tiempo real de estado online/offline.

**1e. Instalación**
- [ ] `monitor install` interactivo (selección de modo).
- [ ] Script `install.sh` que descarga el binario correcto por plataforma/arch.
- [ ] Unidad systemd generada automáticamente en Linux.

**Criterio de salida**: instalar en 2 máquinas reales (o 2 VMs), ver ambas online en el dashboard, apagar una y verla pasar a offline en <60s sin recargar la página.

### Fase 2 — V2: observabilidad y alertas (según sección 42)

- [x] Docker monitoring (collector vía Docker API: contenedores, estado, cpu/mem/red, reinicios). Backend + reporte del Agent completos; sin UI en el dashboard todavía.
- [x] Health checks HTTP/HTTPS/TCP/Ping/DNS + scheduler de ejecución periódica. Ping vía socket ICMP sin privilegios (`surge-ping`); backend + API completos, sin UI en el dashboard todavía.
- [x] Motor de reglas de alertas (umbral + duración, ej. "CPU > 90% for 5 minutes") con evaluación en el scheduler.
- [x] Notificadores: Email (SMTP vía `lettre`), Webhook genérico.
- [x] Agrupación automática de alertas relacionadas en incidentes + timeline.
- [x] Tags en servidores + filtrado. Filtrado por API (`?tags=`) y CLI (`monitor server add`/API `PATCH tags`); sin UI en el dashboard todavía.
- [x] Grupos de servidores. CRUD + membresía por API; sin UI en el dashboard todavía.
- [ ] Vista de logs (Agent/Monitor/Docker) con filtros por fecha/nivel/servidor/texto. **Diferido**: requiere una infraestructura de log-shipping (el Agent no captura ni envía sus propios logs, el Monitor no persiste los suyos) que es un subsistema propio, no una extensión menor de lo ya construido.
- [x] Retención/downsampling real de `metrics` (crudo → agregados) para no crecer sin control.

**Criterio de salida**: ✅ verificado con un test de integración real (`alerting::tests::container_stopped_fires_webhook_and_opens_incident`) — una regla de alerta dispara un webhook real cuando un contenedor se detiene, y aparece agrupada en un incidente con timeline coherente que se resuelve solo cuando el contenedor vuelve.

**Pendiente para cerrar V2 del todo**: UI en el dashboard para Docker/health checks/alertas/incidentes/tags/grupos (todo el backend y la API ya existen), y la vista de logs completa.

### Fase 3 — V3: automatización y seguridad avanzada (según sección 43)

- [ ] Instalación remota de Agent por SSH (subir binario, instalar servicio, generar credenciales, arrancar, health check — flujo de la sección 10).
- [ ] Deployment tracking + correlación heurística deployment↔incidente (ventana de tiempo, sin causalidad automática).
- [ ] Auditoría completa (audit_log) expuesta en UI/CLI.
- [ ] Notificadores Discord y Telegram.
- [ ] Monitorización distribuida: un Monitor puede añadir a otro Monitor como si fuera un Agent especial, reutilizando el protocolo de heartbeat/registro.
- [ ] Sistema de plugins v1 (contrato mínimo: métricas + health check + metadata) con 1–2 plugins de referencia (nginx, postgres).
- [ ] Backup/restore de configuración+datos.
- [ ] Actualizaciones controladas del Agent desde el Monitor.

**Criterio de salida**: desde el dashboard se instala un Agent nuevo en un VPS remoto solo con IP+credenciales SSH, sin tocar la terminal de ese VPS.

### Fase 4 — V4: plataforma (según sección 44)

- [ ] Soporte PostgreSQL como backend alternativo de `store.Store`.
- [ ] Alta disponibilidad / clustering del Monitor.
- [ ] Discovery automático de dispositivos en red local.
- [ ] Integración Kubernetes.
- [ ] API pública + SDK.
- [ ] Marketplace/registro de plugins.

Esta fase se detalla más adelante, cuando V1–V3 estén validadas con uso real — intentar diseñarla ahora sería especular sobre requisitos que todavía no existen.

---

## 4. Estrategia de testing y calidad

- **Unit tests** por crate (`collector`, `store`, `alerting`, `auth`) con `cargo test`, usando `mockall` o fakes manuales para aislar dependencias externas (Docker, red).
- **Tests de integración** Monitor↔Agent usando instancias reales en Docker Compose (2-3 "servers" simulados) — clave para validar heartbeat/offline detection sin depender de infraestructura real; `sqlx` permite además tests contra una SQLite en memoria por caso de test.
- **Contract tests** sobre `shared::model`/`shared::proto` y los endpoints `/api/agent/*` para no romper compatibilidad Agent viejo ↔ Monitor nuevo (relevante desde que existan actualizaciones, sección 37) — `serde` con `#[serde(deny_unknown_fields)]` selectivo ayuda a detectar breaking changes de forma temprana.
- Cobertura mínima no es objetivo en sí mismo; priorizar tests sobre: detección online/offline, evaluación de reglas de alerta, autenticación (sesión/token inválido se rechaza), y migraciones de esquema.

## 5. Riesgos y decisiones pendientes de validar con uso real

- **Volumen de `metrics`**: sin downsampling, SQLite puede degradar con muchos servidores/alta frecuencia. Mitigado en V2, pero vigilar desde el MVP con un intervalo de recolección razonable (15–30s, no 1s).
- **Tauri en Rust**: al compartir lenguaje con el Monitor, el riesgo de "sidecar multiplataforma" que existía con Go desaparece — `monitor-core` se puede consumir como dependencia directa de Tauri. Aun así, para no acoplar el MVP a la complejidad de un build de escritorio, se mantiene la recomendación de que en el MVP el dashboard sea la app web servida por el propio Monitor, y Tauri se añada en V2 como wrapper fino sobre esa misma web + `monitor-core` embebido.
- **Curva de aprendizaje de `sqlx`/async Rust** si el equipo no tiene experiencia previa: mitigar con un spike de 1-2 días en Fase 0 implementando el trait `Store` mínimo (crear servidor, listar servidores) antes de construir el resto de features sobre él.
- **Nombre del proyecto** (sección 49): decisión de producto, no técnica — no bloquea el desarrollo, puede resolverse en paralelo.

---

## 6. Próximos pasos inmediatos

1. Confirmar el ajuste de la Fase 1d (dashboard web-first, Tauri diferido) — si se acepta, simplifica notablemente el MVP.
2. Ejecutar Fase 0 (scaffolding del repo).
3. Empezar Fase 1 por el Agent (collector de métricas es la pieza con menos dependencias y más fácil de probar de forma aislada).
