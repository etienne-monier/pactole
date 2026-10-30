const app = document.querySelector('#app');

async function loadCapabilities() {
  const response = await fetch('/api/v1/capabilities');
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

loadCapabilities()
  .then(({ capabilities }) => {
    app.innerHTML = `
      <main class="pactole-app">
        <h1>Pactole</h1>
        <p>Local read-only web interface.</p>
        <p class="status">Read access: ${capabilities.read ? 'available' : 'unavailable'}</p>
      </main>`;
  })
  .catch((error) => {
    app.innerHTML = `<main class="pactole-app"><h1>Pactole</h1><p class="error">${error.message}</p></main>`;
  });
