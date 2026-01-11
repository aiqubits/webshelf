# WebShelf 容器化部署指南

本文档提供了 WebShelf 项目的 Docker Compose 和 Kubernetes 部署指南。

## 目录

- [Docker Compose 部署](#docker-compose-部署)
- [Kubernetes 部署](#kubernetes-部署)
- [配置说明](#配置说明)
- [故障排查](#故障排查)

## Docker Compose 部署

Docker Compose 包含以下服务：

- **webshelf**: 主应用服务
- **postgres**: PostgreSQL 数据库 (端口 5432)
- **redis**: Redis 缓存 (端口 6379)

所有服务通过 `webshelf-net` 网络连接，并配置了健康检查和自动重启。

#### 启动所有服务

```bash
docker compose up -d
```

#### 查看日志

```bash
docker compose logs -f webshelf
```

#### 停止所有服务

```bash
docker compose down
```

#### 清理数据卷

```bash
docker compose down -v
```

## Kubernetes 部署

### 前置要求

- Kubernetes 集群 (v1.20+)
- kubectl 配置完成
- 可选: NGINX Ingress Controller

### 部署步骤

#### 1. 创建命名空间

```bash
kubectl apply -f k8s/namespace.yaml
```

#### 2. 部署 PostgreSQL

```bash
kubectl apply -f k8s/postgres.yaml
```

#### 3. 部署 Redis

```bash
kubectl apply -f k8s/redis.yaml
```

#### 4. 部署 WebShelf 应用

首先构建 Docker 镜像：

```bash
docker build -t webshelf:latest .
```

如果使用本地集群 (如 minikube, kind)，需要将镜像加载到集群：

```bash
# minikube
minikube image load webshelf:latest

# kind
kind load docker-image webshelf:latest
```

然后部署应用：

```bash
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/webshelf.yaml
```

#### 5. 部署 Ingress (可选)

```bash
kubectl apply -f k8s/ingress.yaml
```

#### 6. 验证部署

```bash
# 查看所有 Pod
kubectl get pods -n webshelf

# 查看服务
kubectl get svc -n webshelf

# 查看 WebShelf 日志
kubectl logs -f deployment/webshelf -n webshelf
```

### 访问应用

#### 通过 NodePort 访问

```bash
# 获取 NodePort 端口
kubectl get svc webshelf-service-nodeport -n webshelf

# 访问应用
curl http://<node-ip>:<node-port>
```

#### 通过 Ingress 访问

如果部署了 Ingress，添加域名解析：

```bash
# 添加到 /etc/hosts
<ingress-ip> webshelf.local

# 访问应用
curl http://webshelf.local
```

### 扩展应用

```bash
# 扩展到 5 个副本
kubectl scale deployment webshelf --replicas=5 -n webshelf
```

### 更新应用

```bash
# 重新构建镜像
docker build -t webshelf:v2.0 .

# 加载镜像到集群
minikube image load webshelf:v2.0

# 更新部署
kubectl set image deployment/webshelf webshelf=webshelf:v2.0 -n webshelf
```

### 清理资源

```bash
kubectl delete namespace webshelf
```

## 配置说明

### ConfigMap

Kubernetes 部署使用 ConfigMap 管理配置，位于 `k8s/configmap.yaml`。

修改配置后重新应用：

```bash
kubectl apply -f k8s/configmap.yaml
kubectl rollout restart deployment/webshelf -n webshelf
```

### 持久化存储

- PostgreSQL: 1Gi PVC
- Redis: 512Mi PVC

### 资源限制

默认资源配置：

**WebShelf:**
- Request: 256Mi RAM, 250m CPU
- Limit: 512Mi RAM, 500m CPU

**PostgreSQL:**
- Request: 256Mi RAM, 250m CPU
- Limit: 512Mi RAM, 500m CPU

**Redis:**
- Request: 128Mi RAM, 100m CPU
- Limit: 256Mi RAM, 200m CPU

## 故障排查

### Pod 无法启动

```bash
kubectl describe pod <pod-name> -n webshelf
kubectl logs <pod-name> -n webshelf
```

### 数据库连接失败

检查 PostgreSQL Pod 状态：

```bash
kubectl get pods -n webshelf -l app=postgres
kubectl logs -l app=postgres -n webshelf
```

### Redis 连接失败

检查 Redis Pod 状态：

```bash
kubectl get pods -n webshelf -l app=redis
kubectl logs -l app=redis -n webshelf
```
