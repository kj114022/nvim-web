# Kubernetes Deployment

Deploy nvim-web with pod-per-session scaling on Kubernetes.

## Architecture

```
                    ┌─────────────────────────────────────────┐
                    │              Kubernetes Cluster          │
                    │                                          │
   Internet ───────►│  ┌─────────┐    ┌──────────────────┐    │
                    │  │ Ingress │───►│  Router Service  │    │
                    │  └─────────┘    └────────┬─────────┘    │
                    │                          │              │
                    │         ┌────────────────┼────────────┐ │
                    │         │                │            │ │
                    │         ▼                ▼            ▼ │
                    │    ┌─────────┐    ┌─────────┐   ┌─────────┐
                    │    │ Session │    │ Session │   │ Session │
                    │    │  Pod 1  │    │  Pod 2  │   │  Pod N  │
                    │    └─────────┘    └─────────┘   └─────────┘
                    │                                          │
                    └─────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- Kubernetes cluster (1.24+)
- kubectl configured
- Container registry access

### Deploy

```bash
# Apply RBAC
kubectl apply -f k8s/rbac.yaml

# Deploy router
kubectl apply -f k8s/router-deployment.yaml
kubectl apply -f k8s/router-service.yaml

# Configure ingress
kubectl apply -f k8s/ingress.yaml
```

## Configuration

### Router Configuration

```toml
[kubernetes]
enabled = true
namespace = "nvim-web"
image = "ghcr.io/your-org/nvim-web-session:latest"
storage_size = "1Gi"
session_timeout_secs = 3600  # 1 hour
max_sessions = 100

[kubernetes.resources]
cpu_request = "100m"
cpu_limit = "1"
memory_request = "128Mi"
memory_limit = "512Mi"
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NVIM_WEB_K8S_ENABLED` | Enable K8s mode | `false` |
| `NVIM_WEB_K8S_NAMESPACE` | Pod namespace | `nvim-web` |
| `NVIM_WEB_K8S_IMAGE` | Session pod image | - |
| `NVIM_WEB_K8S_MAX_SESSIONS` | Max concurrent sessions | `100` |

## Session Lifecycle

### Creation

1. Client requests new session via API
2. Router creates pod with session ID
3. Pod starts nvim-web in session mode
4. Router returns connection info to client
5. Client connects directly to session pod

### Deletion

Sessions are deleted when:
- User explicitly ends session
- Session timeout expires
- Pod becomes unhealthy

### Cleanup

The router periodically cleans up expired sessions:

```rust
// Every 5 minutes
manager.cleanup_expired().await?;
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/sessions` | GET | List active sessions |
| `/sessions` | POST | Create new session |
| `/sessions/{id}` | GET | Get session details |
| `/sessions/{id}` | DELETE | Delete session |
| `/sessions/{id}/connect` | GET | Get connection info |
| `/health` | GET | Health check |

## Storage

### EmptyDir (Default)

Ephemeral storage, lost when pod terminates:

```yaml
volumes:
  - name: workspace
    emptyDir:
      sizeLimit: 1Gi
```

### PersistentVolumeClaim

Retained storage for session recovery:

```yaml
volumes:
  - name: workspace
    persistentVolumeClaim:
      claimName: session-{id}-pvc
```

## Scaling

### Horizontal Pod Autoscaler

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: nvim-web-router
  namespace: nvim-web
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: nvim-web-router
  minReplicas: 2
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
```

## Security

### RBAC

The router needs permissions to manage pods:

```yaml
rules:
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get", "list", "watch", "create", "delete"]
```

### Network Policies

Isolate session pods:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: session-isolation
  namespace: nvim-web
spec:
  podSelector:
    matchLabels:
      app: nvim-web-session
  policyTypes:
    - Ingress
    - Egress
  ingress:
    - from:
        - podSelector:
            matchLabels:
              app.kubernetes.io/component: router
```

## Monitoring

### Prometheus Metrics

The router exposes metrics at `/metrics`:

- `nvim_web_sessions_active` - Current active sessions
- `nvim_web_sessions_created_total` - Total sessions created
- `nvim_web_session_duration_seconds` - Session duration histogram

### Logging

Session pods log to stdout/stderr, collected by your cluster's logging solution.
