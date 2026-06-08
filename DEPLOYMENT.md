# WebShelf 容器化部署指南

本文档介绍 WebShelf 的完整容器化部署方案，包括本地开发、Docker Compose 生产部署和 Kubernetes 集群部署。

## 目录

- [Docker Compose 部署](#docker-compose-部署)
- [Kubernetes 部署](#kubernetes-部署)
- [配置和敏感信息管理](#配置和敏感信息管理)
- [部署最佳实践](#部署最佳实践)
- [故障排查](#故障排查)

---

## Docker Compose 部署

Docker Compose 是快速本地部署和中小型生产环境的推荐方式。完整的编排配置包括：

- **webshelf-server**: 后端服务 (Axum)
- **webshelf-web**: 前端服务 (Dioxus WASM + Nginx)
- **postgres**: PostgreSQL 16 数据库
- **redis**: Redis 7 缓存和分布式锁

所有服务通过 `webshelf-net` 网络连接，配置了健康检查和自动重启。

### 前置要求

- Docker 24.0+
- Docker Compose 2.20+
- 至少 4GB 可用内存
- 至少 5GB 可用磁盘空间

### 部署步骤

#### 1. 准备环境文件

```bash
# 创建 .env 文件
cp .env.example .env

# 编辑 .env，配置以下关键变量：
# WEBSHELF_ENV=production              # 生产环境
# WEBSHELF_JWT_SECRET=<strong-secret>  # 64 字符随机字符串（必需！）
# WEBSHELF_POSTGRES_PASSWORD=<db-password>      # PostgreSQL 密码
# WEBSHELF_REDIS_PASSWORD=<redis-password>      # Redis 密码
```

**关键安全说明:**
- `WEBSHELF_JWT_SECRET` 必需是至少 32 字符的强随机字符串：
  ```bash
  openssl rand -base64 64
  ```
- Redis 密码应为 32 字符的十六进制字符串（避免 URL 特殊字符）：
  ```bash
  openssl rand -hex 32
  ```

#### 2. 配置应用

```bash
# 复制配置模板
cp config.toml.example config.toml

# 编辑 config.toml（可选，环境变量会覆盖大部分配置）
# 重点配置 CORS 允许源（如果使用 HTTPS）：
# [server]
# allowed_origins = ["https://example.com"]
```

#### 3. 启动所有服务

```bash
# 后台启动，输出 compose 日志
docker compose up -d
docker compose logs -f

# 或在前台运行以查看实时日志
docker compose up
```

首次启动会：
- 构建两个容器镜像
- 初始化 PostgreSQL 数据库
- 运行数据库迁移
- 启动所有服务

预期启动时间：2-5 分钟（取决于网络速度）。

#### 4. 验证部署

```bash
# 检查服务状态
docker compose ps

# 检查后端健康
curl http://localhost:3000/api/health

# 检查前端健康
curl http://localhost:80/nginx-health

# 查看特定服务日志
docker compose logs webshelf-server
docker compose logs postgres
docker compose logs redis
```

#### 5. 测试 API

```bash
# 注册新用户
curl -X POST http://localhost:3000/api/public/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "TestPass123",
    "name": "Test User"
  }'

# 登录
curl -X POST http://localhost:3000/api/public/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "TestPass123"
  }'

# 获取 token 后，列表用户（替换 YOUR_TOKEN）
curl http://localhost:3000/api/users \
  -H "Authorization: Bearer YOUR_TOKEN"
```

### Docker Compose 常用命令

```bash
# 查看所有服务状态
docker compose ps

# 查看实时日志
docker compose logs -f

# 查看特定服务日志
docker compose logs -f webshelf-server

# 停止所有服务（保留数据）
docker compose down

# 停止并删除数据（警告：数据会丢失）
docker compose down -v

# 重启特定服务
docker compose restart webshelf-server

# 进入数据库容器
docker compose exec postgres psql -U postgres -d webshelf

# 进入 Redis 容器
docker compose exec redis sh -c 'REDISCLI_AUTH=$REDIS_PASSWORD redis-cli'
# 注意：docker-compose.yml 的 healthcheck 中使用 $$ 是 YAML 转义，
# 但用户手动执行命令时使用单个 $ 即可（如上所示）

# 查看容器内文件系统
docker compose exec webshelf-server /bin/bash
```

### 网络架构

```
┌─────────────────────────────────────────────┐
│         nginx:80 (webshelf-web)             │  反向代理
├─────────────────────────────────────────────┤
│  ↓ proxy to                                 │
│  Backend: 3000 (webshelf-server)            │  API 服务
│  ├─ connects to postgres:5432               │
│  └─ connects to redis:6379                  │
├─────────────────────────────────────────────┤
│  postgres:5432 (webshelf-postgres)          │  数据库
│  redis:6379 (webshelf-redis)                │  缓存/锁
└─────────────────────────────────────────────┘
```

所有容器连接在 `webshelf-net` 网络，使用服务名作为主机名。

### 生产环境配置

对于生产部署，建议配置以下项：

```bash
# .env.production
WEBSHELF_ENV=production
WEBSHELF_JWT_SECRET=REPLACE_ME_WITH_A_STRONG_SECRET
WEBSHELF_POSTGRES_PASSWORD=CHANGE_ME_POSTGRES_PASSWORD
WEBSHELF_REDIS_PASSWORD=CHANGE_ME_REDIS_PASSWORD
```

```toml
# config.toml (生产)
database_url = "postgres://postgres:CHANGE_ME_POSTGRES_PASSWORD@postgres:5432/webshelf"
redis_url = "redis://:CHANGE_ME_REDIS_PASSWORD@redis:6379"
jwt_secret = "REPLACE_ME_WITH_A_STRONG_SECRET"
jwt_expiry_seconds = 3600

[server]
host = "0.0.0.0"
port = 3000
# 必须设置 CORS 允许源
allowed_origins = ["https://example.com", "https://app.example.com"]

[database]
max_connections = 20
min_connections = 5
```

### 数据持久化

Docker Compose 使用命名卷存储数据：

```bash
# 查看卷
docker volume ls | grep webshelf

# 查看卷详细信息
docker volume inspect webshelf_postgres-data

# 备份 PostgreSQL
docker compose exec -T postgres pg_dump -U postgres -d webshelf > backup.sql

# 恢复 PostgreSQL
cat backup.sql | docker compose exec -T postgres psql -U postgres -d webshelf

# 备份 Redis
docker compose exec redis sh -c 'REDISCLI_AUTH=$REDIS_PASSWORD redis-cli BGSAVE'

# 查看 Redis 快照
docker compose exec redis ls -la /data/
```

### 更新部署

```bash
# 拉取最新代码
git pull origin main

# 重新构建镜像
docker compose build

# 执行滚动更新
docker compose up -d

# 验证新版本
docker compose logs webshelf-server | head -20
```

### 性能优化

对于高流量场景：

```toml
# 增加数据库连接池
[database]
max_connections = 50
min_connections = 10

# Redis 持久化优化
# 在 docker-compose.yml redis 服务的 command 中：
# redis-server --requirepass ${WEBSHELF_REDIS_PASSWORD} --appendonly yes --appendfsync everysec
```

---

## Kubernetes 部署

Kubernetes 提供企业级高可用、可扩展部署方案。

### 前置要求

- Kubernetes 集群 (v1.20+)
- kubectl 配置完成
- 可选: Metrics Server (用于自动伸缩)
- 可选: NGINX Ingress Controller (用于外部访问)

### 快速部署

#### 1. 创建命名空间和密钥

```bash
# 创建命名空间
kubectl apply -f k8s/namespace.yaml

# 创建密钥
cp k8s/secret.yaml.example k8s/secret.yaml

# 编辑 k8s/secret.yaml，设置敏感信息：
# - jwt_secret (Base64 编码)
# - database_url (Base64 编码)
# - postgres_password (Base64 编码)
# - redis_password (Base64 编码)
# - redis_url (Base64 编码)

# 应用密钥
kubectl apply -f k8s/secret.yaml
```

**关键 Secret 密钥名称 (base64 编码):**

```bash
# 生成强 JWT 秘钥 (64 个 Base64 字节)
echo -n "$(openssl rand -base64 64)" | base64

# 生成数据库密码
echo -n "your-db-password" | base64

# 生成 Redis 密码 (32 个十六进制)
echo -n "$(openssl rand -hex 32)" | base64

# 批量编码：
echo -n "postgres://postgres:your-password@postgres-service.webshelf.svc.cluster.local:5432/webshelf" | base64
echo -n "redis://:your-password@redis-service.webshelf.svc.cluster.local:6379" | base64
```

#### 2. 部署数据库和缓存

```bash
# 部署 PostgreSQL StatefulSet
kubectl apply -f k8s/postgres.yaml

# 等待 PostgreSQL 就绪
kubectl wait --for=condition=ready pod -l app=postgres -n webshelf --timeout=300s

# 部署 Redis StatefulSet
kubectl apply -f k8s/redis.yaml

# 等待 Redis 就绪
kubectl wait --for=condition=ready pod -l app=redis -n webshelf --timeout=300s
```

#### 3. 构建容器镜像

在 Docker Registry 中构建并推送镜像：

```bash
# 构建镜像（本地 Kubernetes，如 minikube）
docker build -t webshelf-server:latest -f Dockerfile.server .
docker build -t webshelf-web:latest -f Dockerfile.web .

# 如果使用 minikube，加载镜像到集群
minikube image load webshelf-server:latest
minikube image load webshelf-web:latest

# 如果使用远程 Registry（如 Docker Hub, ECR, 等）
docker tag webshelf-server:latest your-registry/webshelf-server:latest
docker tag webshelf-web:latest your-registry/webshelf-web:latest
docker push your-registry/webshelf-server:latest
docker push your-registry/webshelf-web:latest

# 更新 k8s/webshelf.yaml 中的镜像地址：
# image: your-registry/webshelf-server:latest
# image: your-registry/webshelf-web:latest
```

#### 4. 应用配置和部署

```bash
# 应用 ConfigMap（可选，大部分配置通过环境变量）
kubectl apply -f k8s/configmap.yaml

# 部署后端应用
kubectl apply -f k8s/webshelf.yaml

# 等待 WebShelf 部署就绪
kubectl rollout status deployment/webshelf -n webshelf --timeout=300s

# 部署前端（可选，可由后端 Nginx 提供）
kubectl apply -f k8s/webshelf-web.yaml
```

#### 5. 配置 Ingress（外部访问）

```bash
# 应用 Ingress 配置
kubectl apply -f k8s/ingress.yaml

# 获取 Ingress IP/Host
kubectl get ingress webshelf-ingress -n webshelf

# 添加 DNS 记录或 /etc/hosts 条目
echo "ingress-ip webshelf.local" >> /etc/hosts

# 访问应用
curl http://webshelf.local
```

### Kubernetes 网络架构

```
┌────────────────────────────────────────────────────────┐
│                 Ingress (webshelf-ingress)             │  外部入口
│                  webshelf.local:80                     │
├────────────────────────────────────────────────────────┤
│  ↓ 路由到                                              │
│  Frontend: nginx                                       │  SPA 应用
│  webshelf-web-service:80 (2 replicas)                  │
├────────────────────────────────────────────────────────┤
│  ↓ Nginx reverse proxy →                               │
│  Backend: webshelf-service:3000 (3 replicas)          │  API 服务
│  ├─ connects to postgres-service:5432                 │
│  └─ connects to redis-service:6379                    │
├────────────────────────────────────────────────────────┤
│  PostgreSQL StatefulSet: postgres-service:5432        │  数据库
│  postgres-pvc (PersistentVolumeClaim)                 │
├────────────────────────────────────────────────────────┤
│  Redis StatefulSet: redis-service:6379                │  缓存/分布式锁
│  redis-pvc (PersistentVolumeClaim)                    │
└────────────────────────────────────────────────────────┘
```

**关键服务名称 (Service DNS):**
- `webshelf-web-service.webshelf.svc.cluster.local:80` - 前端
- `webshelf-service.webshelf.svc.cluster.local:3000` - 后端
- `postgres-service.webshelf.svc.cluster.local:5432` - 数据库
- `redis-service.webshelf.svc.cluster.local:6379` - Redis

所有服务在 `webshelf` 命名空间内通过 Kubernetes DNS 服务发现。

### 常用 kubectl 命令

```bash
# 查看所有资源
kubectl get all -n webshelf

# 查看 Pod
kubectl get pods -n webshelf
kubectl describe pod webshelf-xxx -n webshelf
kubectl logs pod/webshelf-xxx -n webshelf

# 查看服务
kubectl get svc -n webshelf
kubectl describe svc webshelf-service -n webshelf

# 进入 Pod
kubectl exec -it pod/webshelf-xxx -n webshelf -- /bin/bash

# 查看 StatefulSet
kubectl get statefulset -n webshelf
kubectl describe statefulset postgres -n webshelf

# 查看持久卷
kubectl get pvc -n webshelf
kubectl describe pvc postgres-pvc -n webshelf

# 查看部署历史
kubectl rollout history deployment/webshelf -n webshelf

# 回滚到上一个版本
kubectl rollout undo deployment/webshelf -n webshelf

# 查看事件
kubectl get events -n webshelf --sort-by='.lastTimestamp'
```

### 扩展应用

#### 手动扩展

```bash
# 扩展到 5 个副本
kubectl scale deployment webshelf --replicas=5 -n webshelf

# 查看扩展进度
kubectl rollout status deployment/webshelf -n webshelf
```

#### 自动扩展

```bash
# 创建 HPA (Horizontal Pod Autoscaler)
kubectl autoscale deployment webshelf --min=2 --max=10 \
  --cpu-percent=70 -n webshelf

# 查看 HPA 状态
kubectl get hpa -n webshelf
kubectl describe hpa webshelf-hpa -n webshelf

# 监控资源使用
kubectl top pod -n webshelf
```

### 更新和灰度发布

#### 滚动更新

```bash
# 更新镜像版本
kubectl set image deployment/webshelf \
  webshelf=your-registry/webshelf-server:v2.0 \
  -n webshelf

# 监控更新
kubectl rollout status deployment/webshelf -n webshelf

# 查看部署历史
kubectl rollout history deployment/webshelf -n webshelf
```

#### 金丝雀发布

```bash
# 创建新的 Deployment
kubectl create deployment webshelf-canary --image=your-registry/webshelf-server:v2.0 \
  -n webshelf

# 添加标签用于流量分配
kubectl label deployment webshelf-canary version=canary -n webshelf

# 将 10% 流量指向金丝雀（需要 Istio 或类似的服务网格）
# 或在 Ingress 中配置流量分割

# 验证金丝雀稳定后，更新主部署
kubectl set image deployment/webshelf \
  webshelf=your-registry/webshelf-server:v2.0 \
  -n webshelf

# 删除金丝雀部署
kubectl delete deployment webshelf-canary -n webshelf
```

### 备份和恢复

#### PostgreSQL 备份

```bash
# 创建备份
kubectl exec -it statefulset/postgres -n webshelf -- \
  pg_dump -U postgres -d webshelf > backup.sql

# 恢复备份
kubectl exec -i statefulset/postgres -n webshelf -- \
  psql -U postgres -d webshelf < backup.sql

# 查看持久卷
kubectl get pvc postgres-pvc -n webshelf -o yaml
```

#### Redis 持久化

```bash
# 强制保存
kubectl exec -it statefulset/redis -n webshelf -- sh -c 'REDISCLI_AUTH=$REDIS_PASSWORD redis-cli BGSAVE'

# 备份 RDB 文件
kubectl cp webshelf/redis-0:/data/dump.rdb ./redis-dump.rdb

# 恢复 RDB 文件
kubectl cp ./redis-dump.rdb webshelf/redis-0:/data/dump.rdb
```

### 资源限制

```yaml
# k8s/webshelf.yaml 中的资源限制（已配置）
resources:
  requests:
    memory: "256Mi"
    cpu: "250m"
  limits:
    memory: "512Mi"
    cpu: "500m"
```

调整资源以适应实际负载。

### 监控和日志

#### 查看日志

```bash
# 实时日志
kubectl logs -f deployment/webshelf -n webshelf

# 查看最后 100 行
kubectl logs --tail=100 deployment/webshelf -n webshelf

# 查看特定 Pod 日志
kubectl logs pod/webshelf-xxx -n webshelf

# 跨 Pod 查看日志
kubectl logs -f --all-containers=true deployment/webshelf -n webshelf
```

#### 导出日志用于外部分析

```bash
# 导出最近 1 小时的日志
kubectl logs --since=1h deployment/webshelf -n webshelf > logs.txt

# 导出特定时间范围的日志
kubectl logs --since-time=2026-06-08T10:00:00Z deployment/webshelf -n webshelf
```

---

## 配置和敏感信息管理

### 配置文件优先级

配置值的应用优先级（从低到高）：

1. **code default** (代码中的默认值)
2. **config.toml** (配置文件)
3. **Kubernetes Secret** (k8s 密钥)
4. **环境变量** (最高优先级)

### 环境变量参考

```bash
# 数据库
WEBSHELF_DATABASE_URL=postgres://user:pass@host:port/dbname
WEBSHELF_DATABASE__MAX_CONNECTIONS=20
WEBSHELF_DATABASE__MIN_CONNECTIONS=5

# Redis (注意 URL 编码)
WEBSHELF_REDIS_URL=redis://:password@host:6379

# JWT
WEBSHELF_JWT_SECRET=your-secret-key (>=32 chars)
WEBSHELF_JWT_EXPIRY_SECONDS=3600

# 服务器
WEBSHELF_SERVER__HOST=0.0.0.0
WEBSHELF_SERVER__PORT=3000
WEBSHELF_SERVER__ALLOWED_ORIGINS=https://domain.com,https://app.domain.com

# 数据库密码 (需要传递给 PostgreSQL 容器)
WEBSHELF_POSTGRES_PASSWORD=CHANGE_ME_POSTGRES_PASSWORD

# Redis 密码 (需要传递给 Redis 容器)
WEBSHELF_REDIS_PASSWORD=CHANGE_ME_REDIS_PASSWORD

# 日志
RUST_LOG=info|debug|trace|warn|error

# 环境
WEBSHELF_ENV=development|staging|production
```

### 不同环境中的配置

| 环境 | DATABASE_URL | REDIS_URL | 设置方法 |
|------|---------|----------|--------|
| 本地开发 | localhost:5432 | localhost:6379 (无密码) | config.toml 中直接配置 |
| Docker Compose | postgres:5432 | redis:6379 (带密码) | .env 文件中配置 (WEBSHELF_POSTGRES_PASSWORD, WEBSHELF_REDIS_PASSWORD) |
| Kubernetes | postgres-service:5432 | redis-service:6379 (带密码) | k8s/secret.yaml 中配置 |

### 安全最佳实践

1. **永远不要在代码或 Git 中提交敏感信息**
   - 使用 `.env.example` 作为模板
   - 用 `.env` 和 `secret.yaml` 替代，但**永远不提交**
   - 使用 `.gitignore` 排除敏感文件

2. **使用强随机密钥**
   ```bash
   # JWT 密钥（64 字符 Base64）
   openssl rand -base64 64
   
   # 数据库密码（强密码）
   openssl rand -hex 32
   
   # Redis 密码（十六进制，URL 安全）
   openssl rand -hex 32
   ```

3. **HTTPS/TLS**
   - 在生产环境启用 HTTPS
   - 在 Nginx 配置或 Kubernetes Ingress 中配置 TLS
   - 使用 Let's Encrypt 获取免费证书

4. **访问控制**
   - 限制数据库和 Redis 访问
   - 使用防火墙规则
   - 启用强认证

5. **定期轮换密钥**
   - 实施密钥轮换政策
   - 定期更新 JWT 密钥
   - 监控敏感数据访问

---

## 部署最佳实践

### 高可用性

- **冗余**: 部署多个实例，使用负载均衡
- **健康检查**: 启用 Kubernetes 健康检查和 Docker 健康检查
- **自动恢复**: 配置自动重启策略
- **优雅关闭**: 实现正确的 SIGTERM 处理

### 性能优化

- **连接池**: 调整 PostgreSQL 和 Redis 连接池大小
- **缓存**: 合理使用 Redis 缓存
- **压缩**: 启用 Gzip/Brotli 响应压缩（已启用）
- **CDN**: 为静态资源使用 CDN

### 成本优化

- **资源限制**: 设置合理的 CPU 和内存限制
- **自动扩展**: 根据负载动态伸缩
- **镜像优化**: 使用 slim/alpine 基础镜像
- **存储优化**: 定期清理日志和临时数据

### 安全强化

- **镜像扫描**: 定期扫描容器漏洞
- **权限最小化**: 使用非 root 用户运行容器
- **网络隔离**: 使用网络策略限制流量
- **审计日志**: 启用和监控审计日志

---

## 故障排查

### Pod 无法启动

```bash
# 查看 Pod 详情
kubectl describe pod webshelf-xxx -n webshelf

# 查看 Pod 日志
kubectl logs pod/webshelf-xxx -n webshelf

# 常见问题：
# - CrashLoopBackOff: 应用启动失败，查看日志
# - ImagePullBackOff: 镜像不存在或无权限访问
# - Pending: 资源不足或调度问题
```

### 数据库连接失败

```bash
# 检查 PostgreSQL 状态
kubectl get pod -l app=postgres -n webshelf
kubectl logs statefulset/postgres -n webshelf

# 测试连接
kubectl exec -it pod/webshelf-xxx -n webshelf -- \
  psql -h postgres -U postgres -d webshelf

# 检查密钥是否正确
kubectl get secret webshelf-secret -n webshelf -o yaml
```

### Redis 连接失败

```bash
# 检查 Redis 状态
kubectl get pod -l app=redis -n webshelf
kubectl logs statefulset/redis -n webshelf

# 测试连接
kubectl exec -it pod/redis-0 -n webshelf -- sh -c 'REDISCLI_AUTH=$REDIS_PASSWORD redis-cli ping'

# 检查密码编码
echo -n "your-password" | base64
```

### 高内存或 CPU 使用

```bash
# 查看资源使用
kubectl top pod -n webshelf
kubectl top node

# 查看限制配置
kubectl get pod webshelf-xxx -n webshelf -o yaml | grep -A5 resources

# 增加限制
kubectl set resources deployment/webshelf \
  --limits=cpu=1000m,memory=1Gi \
  --requests=cpu=500m,memory=512Mi \
  -n webshelf
```

### 磁盘空间不足

```bash
# 检查 PVC 使用
kubectl get pvc -n webshelf
kubectl describe pvc postgres-pvc -n webshelf

# 清理过期日志
kubectl exec -it statefulset/postgres -n webshelf -- \
  vacuumdb -U postgres -d webshelf

# 扩展 PVC（如果支持）
kubectl patch pvc postgres-pvc -n webshelf -p \
  '{"spec":{"resources":{"requests":{"storage":"50Gi"}}}}'
```

### 网络连接问题

```bash
# 测试 Pod 间连通性
kubectl exec -it pod/webshelf-xxx -n webshelf -- \
  curl http://postgres:5432

# 检查 DNS 解析
kubectl exec -it pod/webshelf-xxx -n webshelf -- nslookup postgres

# 查看网络策略
kubectl get networkpolicy -n webshelf
```

### Docker Compose 故障

```bash
# 查看容器日志
docker compose logs webshelf-server
docker compose logs postgres

# 重启服务
docker compose restart webshelf-server

# 清理并重新启动
docker compose down
docker compose up -d

# 检查网络
docker network ls
docker network inspect webshelf-net
```

### 性能问题诊断

```bash
# 检查数据库连接数
kubectl exec -it statefulset/postgres -n webshelf -- \
  psql -U postgres -c "SELECT count(*) FROM pg_stat_activity;"

# 检查数据库查询性能
kubectl exec -it statefulset/postgres -n webshelf -- \
  psql -U postgres -c "SELECT * FROM pg_stat_statements LIMIT 10;"

# 监控应用日志中的慢查询
kubectl logs deployment/webshelf -n webshelf | grep "slow"
```

---

## 总结

| 方案 | 适用场景 | 优势 | 劣势 |
|------|---------|------|------|
| **本地开发** | 个人开发环境 | 简单快速 | 无法测试分布式环境 |
| **Docker Compose** | 中小型部署、测试 | 易于部署、快速迭代 | 无法自动扩展 |
| **Kubernetes** | 大规模、高可用部署 | 自动扩展、高可用、自愈 | 复杂度高、学习曲线陡 |

根据您的需求和基础设施选择合适的部署方案。
