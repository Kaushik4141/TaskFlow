import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'

const savedTheme = localStorage.getItem('taskflow.theme')
if (savedTheme && savedTheme !== 'default') {
  document.documentElement.className = `theme-${savedTheme}`
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
