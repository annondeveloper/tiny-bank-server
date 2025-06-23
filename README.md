Standalone Deployment Guide
---------------------------

This guide explains how to deploy the `tiny-bank-server` application and its database using a pre-built image from Docker Hub and a single `docker-compose.yml` file. The API container will automatically handle database migrations on startup.

### Prerequisites

*   [Docker](https://docs.docker.com/get-docker/) and Docker Compose must be installed and running on your system.


### Step 1: Create the `docker-compose.yml` File

1.  Create a new, empty folder for your deployment (e.g., `~/server-deployment`).

2.  Inside that folder, create a file named `docker-compose.yml`.

3.  Copy the content from the `docker-compose.yml` and paste it into this new file.

4.  **Important:** Edit the `environment` section within the `docker-compose.yml` file to replace the placeholder passwords and secrets with your actual production values.


Your final directory structure is extremely simple:

```   
~/server-deployment/  
|-- docker-compose.yml   
```

### Step 2: Run the Application

Now you can start the entire application stack with a single command.

1.  Open a terminal.

2.  Navigate to your deployment directory (e.g., `cd ~/server-deployment`).

3.  Run the `up` command:
```
docker-compose up -d
```
* `up`: This command starts all the services defined in your docker-compose.yml file.
* `\-d`: This flag runs the containers in "detached" mode (in the background).


Docker will now pull your API image from Docker Hub, pull the PostgreSQL image, and start both containers. The API container will wait for the database, run any necessary migrations automatically, and then start the server.

### Step 3: Test the Live Application

Your server is now running and publicly accessible on your machine.

1.  **Open your web browser** and navigate to the interactive API documentation:[**http://localhost:3000/swagger-ui**](http://localhost:3000/swagger-ui)

2.  **Use the Swagger UI or Postman** to send requests to `http://localhost:3000/register` to test the full functionality.


### Managing the Application

* To view live logs from both services:
```
docker-compose logs -f
 ```
* To stop and remove the containers:
```
docker-compose down
```
_(Your database data will be safely stored in a Docker volume and will be available the next time you run `docker-compose up`)_.