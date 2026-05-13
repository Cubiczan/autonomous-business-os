# Deployment

## Railway

1. Create a Railway project.
2. Add Postgres and Redis services.
3. Connect the repository.
4. Set variables from `.env.example`.
5. Railway will use `infra/railway.json` and the Dockerfile.
6. Verify `/health`, `/ready`, `/admin`, and `/metrics`.

## AWS

1. Build and push the Docker image to ECR.
2. Provision RDS Postgres, ElastiCache Redis, and SSM/Secrets Manager values.
3. Create an ECS Fargate service using `infra/aws-ecs-task-definition.example.json`.
4. Put the service behind an ALB with TLS.
5. Scrape `/metrics` with Prometheus, AMP, or a compatible collector.

## GCP

1. Build and push the Docker image to Artifact Registry.
2. Provision Cloud SQL Postgres and Memorystore Redis.
3. Store secrets in Secret Manager.
4. Deploy using `infra/gcp-cloud-run.service.yaml`.
5. Configure Cloud Scheduler or a worker service for background jobs if the API scheduler is
   disabled.

## Database

The starter creates tables automatically on startup. For production, replace this with Alembic
migrations before the first serious customer deployment.
