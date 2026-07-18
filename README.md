# Customer Management Application

This is a full-stack web application for managing customer data, consisting of a Next.js frontend and a Rust backend API.

## Frontend (Next.js)

The frontend is built with Next.js and provides a modern, responsive user interface for managing customer records.

### Features

- Customer list view with pagination
- Search customers by company name
- Sort customers by different fields
- Add new customers
- Edit existing customers 
- Delete customers
- Responsive design

### Technical Details

- Built with Next.js and React
- Material-UI components for consistent styling
- Axios for API communication
- Form validation and error handling
- State management with React hooks
- TypeScript for type safety

## Backend (Rust)

The backend is a RESTful API built with Rust and the Rocket framework that provides customer management functionality. It connects to a SQLite database (Northwind) and implements CRUD operations.

### Features

- Get all customers with pagination, filtering and sorting
- Get a single customer by ID
- Create new customers
- Update existing customers
- Delete customers
- CORS enabled for cross-origin requests

### Technical Details

- Built with Rust and Rocket web framework
- SQLite database with rusqlite for data persistence
- JSON serialization/deserialization with serde
- Thread-safe database access with Mutex
- Error handling and input validation

## Running the Project

### Backend
1. Make sure you have Rust and Cargo installed
2. Navigate to the `back` directory
3. Place the Northwind SQLite database file in the project root
4. Run `cargo run` to start the server
5. API will be available at `http://localhost:8001`

### Frontend
1. Make sure you have Node.js installed
2. Navigate to the `front` directory
3. Install dependencies with `npm install`
4. For development, copy `.env.example` to `.env`
5. Run `npm run dev` to start the development server
6. Access the application at `http://localhost:3000`

