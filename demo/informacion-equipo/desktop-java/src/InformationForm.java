import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Font;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.awt.event.WindowAdapter;
import java.awt.event.WindowEvent;
import java.net.InetAddress;
import java.net.URI;
import java.net.URLEncoder;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.time.LocalDate;
import java.time.format.DateTimeParseException;
import java.time.Duration;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JOptionPane;
import javax.swing.JPanel;
import javax.swing.JTextField;
import javax.swing.SwingUtilities;
import javax.swing.SwingWorker;
import javax.swing.UIManager;

/** Formulario de escritorio de la demo. Compatible con OpenJDK 11. */
public final class InformationForm {
    private final URI operation;
    private final HttpClient client = HttpClient.newBuilder()
            .connectTimeout(Duration.ofSeconds(5)).build();
    private final JFrame window = new JFrame("Información del equipo");
    private final JTextField dateField = new JTextField(LocalDate.now().toString(), 26);
    private final JTextField computerField = new JTextField(computerName(), 26);
    private final JTextField aliasField = new JTextField("Usuario demo", 26);
    private final JButton confirmButton = new JButton("Confirmar");
    private final JLabel status = new JLabel("Revisa o edita los datos antes de confirmar.");
    private boolean sending = false;

    private InformationForm(URI operation) {
        this.operation = operation;
        window.setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        JPanel content = new JPanel(new BorderLayout(0, 22));
        content.setBorder(BorderFactory.createEmptyBorder(26, 28, 24, 28));
        JLabel title = new JLabel("Información del equipo");
        title.setFont(title.getFont().deriveFont(Font.BOLD, 21));
        content.add(title, BorderLayout.NORTH);

        JPanel fields = new JPanel(new GridBagLayout());
        addField(fields, "Fecha", dateField, "date", 0, 'F');
        addField(fields, "Nombre del equipo", computerField, "computerName", 1, 'N');
        addField(fields, "Alias", aliasField, "alias", 2, 'A');
        content.add(fields, BorderLayout.CENTER);

        JPanel footer = new JPanel(new BorderLayout(0, 16));
        status.setFont(status.getFont().deriveFont(12f));
        status.setForeground(new Color(80, 95, 115));
        footer.add(status, BorderLayout.NORTH);
        confirmButton.setName("confirm");
        confirmButton.getAccessibleContext().setAccessibleName("Confirmar");
        confirmButton.setMnemonic('C');
        confirmButton.addActionListener(event -> confirm());
        footer.add(confirmButton, BorderLayout.SOUTH);
        content.add(footer, BorderLayout.SOUTH);
        window.setContentPane(content);
        window.getRootPane().setDefaultButton(confirmButton);
        window.addWindowListener(new WindowAdapter() {
            @Override
            public void windowClosing(WindowEvent event) {
                cancel();
            }
        });
        window.pack();
        window.setMinimumSize(window.getSize());
        window.setLocationRelativeTo(null);
        window.setVisible(true);
    }

    private static void addField(JPanel panel, String text, JTextField field,
                                 String name, int row, char mnemonic) {
        JLabel label = new JLabel(text);
        label.setLabelFor(field);
        label.setDisplayedMnemonic(mnemonic);
        field.setName(name);
        field.getAccessibleContext().setAccessibleName(text);
        GridBagConstraints layout = new GridBagConstraints();
        layout.gridx = 0;
        layout.gridy = row * 2;
        layout.weightx = 1;
        layout.anchor = GridBagConstraints.WEST;
        layout.fill = GridBagConstraints.HORIZONTAL;
        layout.insets = new Insets(0, 0, 7, 0);
        panel.add(label, layout);
        layout.gridy++;
        layout.insets = new Insets(0, 0, 18, 0);
        panel.add(field, layout);
    }

    private static String computerName() {
        String name = System.getenv("COMPUTERNAME");
        if (name != null && !name.isBlank()) return name;
        try {
            return InetAddress.getLocalHost().getHostName();
        } catch (Exception error) {
            return "Equipo local";
        }
    }

    private Map<String, String> values() {
        Map<String, String> values = new LinkedHashMap<>();
        String date = dateField.getText().strip();
        try {
            if (!LocalDate.parse(date).toString().equals(date)) throw new DateTimeParseException("", date, 0);
        } catch (DateTimeParseException error) {
            throw new IllegalArgumentException("La fecha debe ser válida y tener formato YYYY-MM-DD.");
        }
        values.put("date", date);
        values.put("computerName", textValue(computerField, "nombre del equipo"));
        values.put("alias", textValue(aliasField, "alias"));
        return values;
    }

    private static String textValue(JTextField field, String name) {
        String value = field.getText().strip();
        int count = value.codePointCount(0, value.length());
        if (count < 1 || count > 100 || value.codePoints().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException("El " + name + " debe contener entre 1 y 100 caracteres visibles.");
        }
        return value;
    }

    private void confirm() {
        if (sending) return;
        final Map<String, String> data;
        try {
            data = values();
        } catch (IllegalArgumentException error) {
            status.setText(error.getMessage());
            return;
        }
        setSending(true, "Enviando los datos a la web…");
        new SwingWorker<Void, Void>() {
            @Override
            protected Void doInBackground() throws Exception {
                post("/result", data);
                return null;
            }

            @Override
            protected void done() {
                try {
                    get();
                    window.dispose();
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    setSending(false, "El envío se interrumpió. Puedes reintentar.");
                } catch (ExecutionException error) {
                    setSending(false, "No se pudo enviar. Revisa que la web siga abierta y reintenta.");
                    JOptionPane.showMessageDialog(window, error.getCause().getMessage(),
                            "No se pudo confirmar", JOptionPane.ERROR_MESSAGE);
                }
            }
        }.execute();
    }

    private void cancel() {
        if (sending) return;
        setSending(true, "Cancelando la operación…");
        new SwingWorker<Void, Void>() {
            @Override
            protected Void doInBackground() throws Exception {
                post("/cancel", Map.of());
                return null;
            }

            @Override
            protected void done() {
                try {
                    get();
                    window.dispose();
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                    setSending(false, "La cancelación se interrumpió.");
                } catch (ExecutionException error) {
                    setSending(false, "No se pudo cancelar. Revisa la conexión con la web.");
                    int choice = JOptionPane.showConfirmDialog(window,
                            "La web no recibió la cancelación. ¿Cerrar la ventana de todas formas?",
                            "Cerrar aplicación", JOptionPane.YES_NO_OPTION);
                    if (choice == JOptionPane.YES_OPTION) window.dispose();
                }
            }
        }.execute();
    }

    private void setSending(boolean value, String message) {
        sending = value;
        confirmButton.setEnabled(!value);
        for (JTextField field : new JTextField[] {dateField, computerField, aliasField}) field.setEnabled(!value);
        status.setText(message);
    }

    private void post(String suffix, Map<String, String> data) throws Exception {
        StringBuilder body = new StringBuilder();
        for (Map.Entry<String, String> entry : data.entrySet()) {
            if (body.length() > 0) body.append('&');
            body.append(URLEncoder.encode(entry.getKey(), StandardCharsets.UTF_8));
            body.append('=');
            body.append(URLEncoder.encode(entry.getValue(), StandardCharsets.UTF_8));
        }
        HttpRequest request = HttpRequest.newBuilder(URI.create(operation.toString() + suffix))
                .timeout(Duration.ofSeconds(20))
                .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
                .POST(HttpRequest.BodyPublishers.ofString(body.toString(), StandardCharsets.UTF_8)).build();
        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        if (response.statusCode() / 100 != 2) {
            throw new IllegalStateException("La web rechazó la petición (HTTP " + response.statusCode()
                    + "). " + response.body());
        }
    }

    public static void main(String[] args) {
        try {
            if (args.length != 1) throw new IllegalArgumentException("Abre la aplicación desde el botón Obtener información de la web.");
            URI operation = URI.create(args[0]);
            if (!"http".equals(operation.getScheme())
                    || !("127.0.0.1".equals(operation.getHost()) || "localhost".equals(operation.getHost()))
                    || operation.getRawQuery() != null || operation.getRawFragment() != null
                    || operation.getUserInfo() != null
                    || !operation.getPath().matches("/api/operations/[A-Za-z0-9_-]{20,64}")) {
                throw new IllegalArgumentException("La operación debe pertenecer al servidor local de la demo.");
            }
            UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
            SwingUtilities.invokeLater(() -> new InformationForm(operation));
        } catch (Exception error) {
            JOptionPane.showMessageDialog(null, error.getMessage(), "No se pudo abrir la demo", JOptionPane.ERROR_MESSAGE);
            System.exit(1);
        }
    }
}
