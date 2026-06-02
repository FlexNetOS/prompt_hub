# Deployment Guide

## Blue/Green Deployment
1. Deploy green version alongside blue
2. Run health checks for 60s
3. Switch traffic if /health passes
4. Keep blue for instant rollback

## Docker
```bash
docker build -t prompthub .
docker run -p 8080:8080 -v prompthub-data:/data prompthub
```

## Kubernetes
Apply manifests in k8s/ directory:
- deployment.yaml
- service.yaml
- ingress.yaml
- hpa.yaml
