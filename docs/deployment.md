# WebShelf 部署指南

本文档介绍 WebShelf 的三级部署方案：本地开发、Docker Compose 中小型部署、Kubernetes 高可用部署（含 CloudNativePG 读写分离集群）。

---

## 目录

- [部署方案对比](#部署方案对比)
- [本地开发](#本地开发)
- [Docker Compose 部署](#docker-compose-部署)
- [Kubernetes 部署](#kubernetes-部署)
  - [单实例 PG（开发用）](#单实例-pg开发用)
  - [CloudNativePG 主从集群（生产用）](#cloudnativepg-主从集群生产用)
- [AutoRouter 读写分离配置](#autorouter-读写分离配置)
- [配置和敏感信息管理](#配置和敏感信息管理)
- [部署最佳实践](#部署最佳实践)
- [故障排查](#故障排查)

---

## 部署方案对比

| 方案 | 适用场景 | DB 拓扑 | 应用副本 | 优势 |
|------|---------|---------|---------|------|
| **本地开发** | 个人开发 | 单实例 PG + Redis | 1 | 快速启动，热重载 |
| **Docker Compose** | 中小型/测试 | 单实例 PG + Redis | 1 | 一键部署，可复现 |
| **K8s 单 PG** | 开发/预发布 | 单实例 PG StatefulSet | 3 | 应用层高可用 |
| **K8s CNP 集群** | 生产环境 | 1 主 2 从 + 自动故障转移 | 3 | 全链路高可用 |

---

## 本地开发

### 前置要求

- **Rust** 1.92+
- **Docker** 24.0+（用于启动 PostgreSQL 和 Redis）
- **Dioxus CLI** 0.7.9+（前端开发，可选）

```bash
cargo binstall dioxus-cli --version 0.7.9 -y
```

### 快速开始

```bash
# 1. Docker 网络 & 数据库
docker network create webshelf-net 2>/dev/null || true
docker run -d --network webshelf-net --name webshelf-postgres-dev \
  -e POSTGRES_PASSWORD=devpassword -e POSTGRES_DB=webshelf \
  -v webshelf-postgres-data:/var/lib/postgresql/data \
  -p 5432:5432 postgres:16-alpine
docker run -d --network webshelf-net --name webshelf-redis-dev \
  -v webshelf-redis-data:/data -p 6379:6379 redis:7-alpine

# 2. 配置
cp -n config.toml.example config.toml
sed -i 's/CHANGE_ME_POSTGRES_PASSWORD/devpassword/g' config.toml
sed -i 's|redis://:CHANGE_ME_REDIS_PASSWORD@|redis://|g' config.toml

# 3. 启动（默认 Axum 引擎）
cargo run --package webshelf-server -- --env development --log-level debug

# 4. 切换 Salvo 引擎（可选）
cargo run --package webshelf-server --no-default-features --features webshelf-salvo \
  -- --env development --log-level debug
```

### 验证

```bash
curl http://127.0.0.1:3000/api/health
# → {"status":"ok","version":"0.1.0"}

# 可选的 Dioxus 前端
cd app/web && dx serve --package web --platform web --hot-reload true
```

### 一键脚本

将以上步骤合并为 `start-dev.sh`，内容见 Docker Compose 章节的脚本模式。

---

## Docker Compose 部署

Docker Compose 完整编排以下服务：

| 服务 | 容器 | 端口 | 依赖 |
|------|------|------|------|
| webshelf-server | 后端 (Axum) | 3000 | postgres, redis |
| webshelf-web | Nginx + WASM | 80 | webshelf-server |
| postgres | PostgreSQL 16 | 5432 | - |
| redis | Redis 7 | 6379 | - |

### 快速开始

```bash
# 生成安全密钥
JWT_SECRET=$(openssl rand -base64 64 | tr -d '\n')
DB_PASS=$(openssl rand -hex 32)
REDIS_PASS=$(openssl rand -hex 32)

cat > .env << EOF
WEBSHELF_ENV=production
WEBSHELF_JWT_SECRET=${JWT_SECRET}
WEBSHELF_POSTGRES_PASSWORD=${DB_PASS}
WEBSHELF_REDIS_PASSWORD=${REDIS_PASS}
EOF

cp config.toml.example config.toml
docker compose up -d

# 验证
curl http://127.0.0.1:3000/api/health
curl http://127.0.0.1/nginx-health
```

### 网络架构

```
用户 → nginx:80 (webshelf-web)
         ├── webshelf-server:3000 → postgres:5432
         └──                      → redis:6379
```

所有容器在 `webshelf-net` 网络内通过服务名互访。

### 数据持久化

```bash
# 备份 PG
docker compose exec -T postgres pg_dump -U postgres -d webshelf > backup.sql
# 恢复
cat backup.sql | docker compose exec -T postgres psql -U postgres -d webshelf

# Redis 持久化（AOF 模式需在 command 中添加 --appendonly yes）
docker compose exec redis redis-cli BGSAVE
```

### 更新

```bash
git pull && docker compose build && docker compose up -d
```

---

## Kubernetes 部署

K8s 部署提供两种数据库拓扑，按环境选择。

| 配置文件 | DB 拓扑 | 用途 |
|----------|---------|------|
| `k8s/postgres.yml` | 单实例 StatefulSet + PVC | 开发/预发布 |
| `k8s/postgres-cluster.yml` | CNP 1 主 2 从 | 生产高可用 |

### 前置要求

- Kubernetes 1.20+ 集群
- kubectl 已配置
- [CloudNativePG Operator](https://cloudnative-pg.io/)（使用 CNP 集群时需要）

### 通用步骤（两种拓扑共享）

#### 1. 命名空间和密钥

```bash
kubectl apply -f k8s/namespace.yml

JWT_SECRET=$(openssl rand -base64 64 | tr -d '\n')
POSTGRES_PASS=$(openssl rand -hex 32)
REDIS_PASS=$(openssl rand -hex 32)

# 单实例 PG 的 URL
DATABASE_URL="postgres://postgres:${POSTGRES_PASS}@postgres-service.webshelf.svc.cluster.local:5432/webshelf"
# CNP 集群的 URL（需要同时配置读库地址）
DATABASE_RO_URL="postgres://postgres:${POSTGRES_PASS}@postgres-cluster-ro.webshelf.svc.cluster.local:5432/webshelf"

REDIS_URL="redis://:${REDIS_PASS}@redis-service.webshelf.svc.cluster.local:6379"

cat > k8s/secret.yml << YAMLEOF
apiVersion: v1
kind: Secret
metadata:
  name: webshelf-secrets
  namespace: webshelf
type: Opaque
data:
  jwt_secret: $(echo -n "$JWT_SECRET" | base64 | tr -d '\n')
  postgres_password: $(echo -n "$POSTGRES_PASS" | base64 | tr -d '\n')
  redis_password: $(echo -n "$REDIS_PASS" | base64 | tr -d '\n')
  database_url: $(echo -n "$DATABASE_URL" | base64 | tr -d '\n')
  redis_url: $(echo -n "$REDIS_URL" | base64 | tr -d '\n')
YAMLEOF

kubectl apply -f k8s/secret.yml
```

#### 2. 构建镜像（Minikube 示例）

```bash
eval $(minikube docker-env)
docker build -t webshelf-server:latest -f Dockerfile.server .
docker build -t webshelf-web:latest -f Dockerfile.web .
minikube image load webshelf-server:latest webshelf-web:latest
```

#### 3. 部署 Redis 和 ConfigMap

```bash
kubectl apply -f k8s/redis.yml
kubectl apply -f k8s/configmap.yml
kubectl wait --for=condition=ready pod -l app=redis -n webshelf --timeout=120s
```

### 单实例 PG（开发用）

适用于开发/预发布环境。使用 `k8s/postgres.yml` 部署单实例 PostgreSQL StatefulSet。

```bash
kubectl apply -f k8s/postgres.yml
kubectl wait --for=condition=ready pod -l app=postgres -n webshelf --timeout=300s
```

此时数据库只有单一端点 `postgres-service:5432`，无需配置 `database_read_urls`。

### CloudNativePG 主从集群（生产用）

#### 安装 CloudNativePG Operator

```bash
# 方式一：直接安装最新版本
kubectl apply --server-side -f \
  https://raw.githubusercontent.com/cloudnative-pg/cloudnative-pg/release-1.24/releases/cnpg-1.24.1.yaml

# 方式二：Helm
helm repo add cnpg https://cloudnative-pg.github.io/charts
helm upgrade --install cnpg cnpg/cloudnative-pg --namespace cnpg-system --create-namespace

# 验证 Operator 就绪
kubectl wait --for=condition=ready pod -l app.kubernetes.io/name=cloudnative-pg \
  -n cnpg-system --timeout=120s
```

#### 部署 CNP 集群

```bash
kubectl apply -f k8s/postgres-cluster.yml

# 自定义存储类（如 Local SSD）：
# kubectl apply -f k8s/postgres-cluster.yml --overwrite=false

kubectl wait --for=condition=ready cluster -n webshelf postgres-cluster --timeout=300s
```

**集群拓扑**（定义于 [k8s/postgres-cluster.yml](../k8s/postgres-cluster.yml)）：

```yaml
spec:
  instances: 3                    # 1 primary + 2 replicas
  storage:
    size: 1Gi
  postgresql:
    parameters:
      max_connections: "100"
      shared_buffers: "256MB"
      hot_standby_feedback: "on"  # 防止主库 vacuum 清理读库正在使用的元组
```

CNP Operator 自动创建三个 Service 端点：

| Service | 用途 | 对应 AutoRouter 配置 |
|---------|------|---------------------|
| `postgres-cluster-rw` | 读写（主库） | `database_url` |
| `postgres-cluster-ro` | 只读（从库） | `database_read_urls` |
| `postgres-cluster-r` | 随机读写 | 内部使用 |

**故障转移**：主库宕机时，Operator 自动提升一个从库为新主，`-rw` Service 自动指向新主。

#### 更新密钥（CNP 模式）

```bash
DATABASE_URL="postgres://postgres:${POSTGRES_PASS}@postgres-cluster-rw.webshelf.svc.cluster.local:5432/webshelf"
# 注意：database_read_urls 在 ConfigMap 中配置，非 Secret
kubectl create secret generic webshelf-secrets --dry-run=client -o yaml \
  --from-literal=jwt_secret="$JWT_SECRET" \
  --from-literal=postgres_password="$POSTGRES_PASS" \
  --from-literal=redis_password="$REDIS_PASS" \
  --from-literal=database_url="$DATABASE_URL" \
  --from-literal=redis_url="$REDIS_URL" \
  -n webshelf | kubectl apply -f -
```

### 部署应用

```bash
# 3 副本后端应用
kubectl apply -f k8s/webshelf.yml
kubectl rollout status deployment/webshelf -n webshelf --timeout=300s

# 前端 Nginx（可选）
kubectl apply -f k8s/webshelf-web.yml

# Ingress
kubectl apply -f k8s/ingress.yml
```

**关键配置**（[k8s/webshelf.yml](../k8s/webshelf.yml)）：

```yaml
spec:
  replicas: 3
  template:
    spec:
      containers:
        - env:
          - name: WEBSHELF_DATABASE_URL
            valueFrom:
              secretKeyRef:
                name: webshelf-secrets
                key: database_url
          - name: WEBSHELF_REDIS_URL
            valueFrom:
              secretKeyRef:
                name: webshelf-secrets
                key: redis_url
          # 使用 CNP 集群时，ConfigMap 中注入 database_read_urls
          envFrom:
          - configMapRef:
              name: webshelf-config
```

### K8s 网络架构

#### 单实例 PG

```
Ingress → webshelf-web:80 (2 replicas)
           → webshelf-server:3000 (3 replicas)
               → postgres-service:5432 (1 primary)
               → redis-service:6379
```

#### CNP 集群模式

```
Ingress → webshelf-web:80 (2 replicas)
           → webshelf-server:3000 (3 replicas)
               → postgres-cluster-rw:5432 (writes)
               → postgres-cluster-ro:5432 (reads, L4 LB)
               → redis-service:6379
```

### 扩展和灰度

```bash
# 手动扩展
kubectl scale deployment webshelf --replicas=5 -n webshelf

# HPA 自动伸缩
kubectl autoscale deployment webshelf --min=3 --max=10 --cpu-percent=70 -n webshelf

# 滚动更新
kubectl set image deployment/webshelf webshelf=your-registry/webshelf-server:v2 -n webshelf
kubectl rollout status deployment/webshelf -n webshelf

# 金丝雀发布（需要服务网格）
kubectl create deployment webshelf-canary --image=your-registry/webshelf-server:v2 -n webshelf
```

---

## AutoRouter 读写分离配置

当配置了读库地址后，`AutoRouter` 自动启用读写分离。业务代码零改动。

### 配置项

```toml
# 单 URL → 无读写分离，所有操作走同一个连接
database_url = "postgres://user:pass@primary:5432/webshelf"

# 多 URL → 启用 AutoRouter
database_url = "postgres://user:pass@primary:5432/webshelf"
database_read_urls = [
  "postgres://user:pass@replica1:5432/webshelf",
  "postgres://user:pass@replica2:5432/webshelf",
]

# 读写分离行为（可选，以下为默认值）
[database_routing]
strategy = "round_robin"       # round_robin | random | weighted
retry_attempts = 2             # 读库失败重试次数
circuit_break_ms = 30000       # 熔断持续时间（毫秒）
fallback_to_write = true       # 所有读库熔断时降级回写库
health_check_interval_secs = 15 # 熔断恢复探测间隔
```

### Docker Compose 模式

```bash
# docker-compose.yml 中只有一个 PG 实例，不启用读写分离
# 如需在本地测试读写分离，添加第二个 PG 容器：
docker compose up -d postgres-replica
```

### K8s CNP 集群模式

```bash
# ConfigMap 中配置读库端点
kubectl patch configmap webshelf-config -n webshelf -p \
  '{"data":{"WEBSHELF_DATABASE_READ_URLS":"postgres://postgres:${POSTGRES_PASS}@postgres-cluster-ro.webshelf.svc.cluster.local:5432/webshelf"}}'
kubectl rollout restart deployment/webshelf -n webshelf
```

### 熔断器行为

| 状态 | 行为 |
|------|------|
| 读库连接失败 | 标记熔断 `down_until`（默认 30s） |
| 后台健康检查 | 每 15s 探测，成功自动恢复 |
| 所有读库熔断 | `fallback_to_write=true` → 降级写库 |
| `fallback_to_write=false` | 读操作返回错误 |

---

## 配置和敏感信息管理

### 优先级（从低到高）

1. 代码默认值 → 2. config.toml → 3. K8s Secret → 4. 环境变量（最高）

### 环境变量参考

```bash
# 数据库
WEBSHELF_DATABASE_URL=postgres://user:pass@host:port/dbname
WEBSHELF_DATABASE_READ_URLS=postgres://user:pass@replica1:5432/dbname  # 逗号分隔或数组 JSON

# 数据库路由（JSON 编码）
WEBSHELF_DATABASE_ROUTING={"strategy":"round_robin","retry_attempts":2,"circuit_break_ms":30000}

# Redis
WEBSHELF_REDIS_URL=redis://:password@host:6379

# JWT
WEBSHELF_JWT_SECRET=your-secret-key   # >= 32 chars
WEBSHELF_JWT_EXPIRY_SECONDS=3600

# 服务器
WEBSHELF_SERVER__HOST=0.0.0.0
WEBSHELF_SERVER__PORT=3000
WEBSHELF_SERVER__ALLOWED_ORIGINS=https://domain1.com,https://domain2.com

# 日志
RUST_LOG=info|debug|trace

# 环境
WEBSHELF_ENV=development|production
```

### 安全最佳实践

```bash
# 生成安全密钥
JWT_SECRET=$(openssl rand -base64 64)        # JWT 签名密钥
DB_PASS=$(openssl rand -hex 32)              # 数据库密码
REDIS_PASS=$(openssl rand -hex 32)           # Redis 密码（hex 保证 URL 安全）

# 不要将 .env / secret.yml / config.toml 提交到 Git
echo ".env" >> .gitignore
echo "k8s/secret.yml" >> .gitignore
```

---

## 部署最佳实践

### 高可用性

- **最小 3 副本**：`k8s/webshelf.yml` 默认 `replicas: 3`，容忍单节点故障
- **Pod 反亲和**：避免同一节点多个副本
- **优雅关闭**：`SIGTERM` 处理，完成进行中的请求后退出

### 性能优化

- **连接池调优**：生产环境 `max_connections=50, min_connections=10`
- **Redis AOF**：在 redis 命令中添加 `--appendonly yes --appendfsync everysec`
- **PG 参数**：CNP 集群中调优 `shared_buffers` / `work_mem`

### 安全强化

- **HTTPS**：在 Ingress 配置 Let's Encrypt（cert-manager）
- **非 root 运行**：Dockerfile 使用 `USER webshelf`
- **NetworkPolicy**：限制 webshelf 命名空间内流量

---

## 故障排查

### PG 连接失败——单实例

```bash
kubectl exec -it pod/webshelf-xxx -n webshelf -- \
  psql -h postgres-service -U postgres -d webshelf
```

### PG 连接失败——CNP 集群

```bash
# 检查集群状态
kubectl cnpg status postgres-cluster -n webshelf

# 检查 Service 端点
kubectl get endpoints -n webshelf | grep postgres-cluster

# 获取主库 Pod 名称
kubectl get pod -n webshelf -l postgresql=postgres-cluster \
  -o jsonpath='{.items[?(@.metadata.labels.role=="primary")].metadata.name}'

# 手动测试连接
kubectl exec -it pod/webshelf-xxx -n webshelf -- \
  sh -c 'apt-get update && apt-get install -y postgresql-client && \
  psql "$WEBSHELF_DATABASE_URL" -c "SELECT 1"'

kubectl exec -it pod/webshelf-xxx -n webshelf -- \
  sh -c 'psql -h postgres-cluster-ro -U postgres -d webshelf -c "SELECT 1"'
```

### Redis 连接失败

```bash
kubectl exec -it pod/redis-0 -n webshelf -- \
  sh -c 'redis-cli -a "$REDIS_PASSWORD" ping'
```

### 读写分离不生效

```bash
# 检查 ConfigMap 是否包含 database_read_urls
kubectl get configmap webshelf-config -n webshelf -o yaml

# 检查应用日志是否显示 AutoRouter 初始化
kubectl logs deployment/webshelf -n webshelf | grep -i "router\|read\|replica"
```

### Pod CrashLoopBackOff

```bash
kubectl describe pod webshelf-xxx -n webshelf | grep -A10 Events
kubectl logs pod/webshelf-xxx -n webshelf --previous
```

---

## 系统要求

| 组件 | 最低版本 | 推荐 |
|------|---------|------|
| Rust | 1.92 | 最新 stable |
| PostgreSQL | 16 | 16-alpine（容器） |
| Redis | 7 | 7-alpine（容器） |
| Docker | 24.0 | 最新 |
| Kubernetes | 1.20 | 1.28+ |
| CloudNativePG | 1.24 | 1.24+ |