Standalone Deployment Guide
---------------------------

This guide explains how to deploy the tiny-bank-server application and its database using a pre-built image from Docker
Hub and a single docker-compose.yml file.

### Prerequisites

* [Docker](https://docs.docker.com/get-docker/) and Docker Compose must be installed and running on your system.

### Step 1: Create the Project Structure

You need to create a specific folder structure for the deployment files.

1. Create a new, empty folder for your deployment. For example: C:\\server-deployment.

2. Inside that folder, create the docker-compose.yml file by copying the content from the document above.

3. Inside that same folder, create a new sub-folder named db.

4. Inside the db folder, create another su b-folder named init.

Your final directory structure should look like this:

```   
/server-deployment/  
|-- db/  
|   |-- init/  
|-- docker-compose.yml   
```

### Step 2: Create the Database Initialization Script

The database container will automatically create the required users table on its first startup if it finds a .sql file
in the db/init directory.

1. Inside the db/init folder, create a new file named 01-init.sql.

2.  ```
    -- This script will be executed automatically when the PostgreSQL container starts.
    -- It creates the 'users' table required by the application.
    CREATE TABLE IF NOT EXISTS users (
      id UUID PRIMARY KEY,
      account_number TEXT NOT NULL UNIQUE,
      ifsc_code TEXT NOT NULL,
      bank_name TEXT NOT NULL,
      branch TEXT NOT NULL,
      address TEXT,
      city TEXT,
      state_code TEXT,
      routing_no TEXT,
      created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );

### Step 3: Run the Application

Now you can start the entire application stack with a single command.

1. Open a terminal (PowerShell or Command Prompt).

2. Navigate to your deployment directory (e.g., cd C:\\server-deployment).

3. docker-compose up -dDocker will now pull your API image from Docker Hub, pull the PostgreSQL image, and start both
   containers.

    * up: This command starts all the services defined in your docker-compose.yml file.

    * \-d: This flag runs the containers in "detached" mode (in the background).

### Step 4: Test the Live Application

Your server is now running and publicly accessible on your machine.

1. **Open your web browser** and navigate to the interactive API documentation:[**http://localhost:3000/swagger-ui
   **](http://localhost:3000/swagger-ui)

2. **Use the Swagger UI or Postman** to send requests to http://localhost:3000/register to test the full functionality.

### Managing the Application

*To view live logs from both services:

```
docker-compose logs -f
```

* To stop and remove the containers:
  ```
  docker-compose down
  ```
  _(Your database data will be safely stored in a Docker volume and will be available the next time you run
  docker-compose up)_.
