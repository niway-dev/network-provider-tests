<!--
ISP complaint template (Spanish). Fill the [placeholders] with values from a
failing `nettest` run before sending. Keep the evidence factual: dates, times,
which probes failed, and the last hop the trace reached inside the ISP network.
-->

**Asunto:** Fallas intermitentes de conexión IPv4 – persisten después de reiniciar el router [N] veces

Buenos días:

Les escribo para reportar fallas intermitentes en mi servicio de internet. Ya reinicié el router [N] veces y el problema sigue.

**Titular:** [nombre]
**Número de cliente / contrato:** [número]
**Dirección:** [dirección]

**Qué está pasando**

La conexión por IPv4 se corta varias veces durante el día. Cuando falla, la mayoría de las páginas no cargan o tardan mucho, aunque el router y la red de mi casa funcionan bien: la respuesta del router es normal y no hay pérdida de paquetes.

**Pruebas realizadas el [fecha]**

1. **Primera falla ([hora]):** las conexiones por IPv4 a Google, GitHub y Cloudflare no respondían. En cambio, el IPv6 funcionaba con normalidad. El traceroute hacia 1.1.1.1 avanzaba por su red interna ([saltos, p. ej. 192.168.1.1 → 10.x.x.x → …]) y **se cortaba después del salto [último salto que respondió]**, sin llegar al destino.
2. **Después de reiniciar el router:** el servicio volvió por unos minutos, pero el ping a 8.8.8.8 seguía perdiendo el [X]% de los paquetes.
3. **Nueva prueba ([hora]):** las conexiones a Google, GitHub, Cloudflare y 1.1.1.1 no responden en 8 segundos [por IPv4 / ni por IPv4 ni por IPv6]. El traceroute muestra saltos sin respuesta dentro de su red, justo después de [último salto que respondió].

Como la red interna de mi casa funciona bien y la falla aparece dentro de su red, entiendo que el problema está de su lado. Puede ser el equipo del salto [último salto que respondió] o el sistema que comparte direcciones IPv4 entre clientes (CGNAT).

**Lo que solicito**

- Que revisen la ruta y el equipo después del salto [último salto que respondió].
- Que revisen mi asignación de IPv4 / CGNAT.
- Si es necesario, una visita técnica.
- Un número de ticket para hacer seguimiento.

Si necesitan más datos o que repita alguna prueba, quedo atento.

Saludos,
[nombre]
[teléfono de contacto]
