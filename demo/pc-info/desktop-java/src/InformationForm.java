import java.awt.BorderLayout;
import java.awt.Color;
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
import java.time.Duration;
import java.time.LocalDate;
import java.time.format.DateTimeParseException;
import java.util.LinkedHashMap;
import java.util.List;
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
    private static final String WINDOW_TITLE = "Información del equipo";
    private static final String DATE_LABEL = "Fecha";
    private static final String COMPUTER_NAME_LABEL = "Nombre del equipo";
    private static final String ALIAS_LABEL = "Alias";
    private static final String CONFIRM_LABEL = "Confirmar";
    private static final String DEFAULT_ALIAS = "Usuario demo";
    private static final String INITIAL_STATUS = "Revisa o edita los datos antes de confirmar.";
    private static final String FIELD_DATE = "date";
    private static final String FIELD_COMPUTER_NAME = "computerName";
    private static final String FIELD_ALIAS = "alias";
    private static final String CONFIRM_CONTROL_NAME = "confirm";
    private static final String RESULT_PATH_SUFFIX = "/result";
    private static final String CANCEL_PATH_SUFFIX = "/cancel";
    private static final String COMPUTER_NAME_ENV = "COMPUTERNAME";
    private static final String DEFAULT_COMPUTER_NAME = "Equipo local";
    private static final String ALLOWED_SCHEME = "http";
    private static final List<String> ALLOWED_HOSTS = List.of("127.0.0.1", "localhost");
    private static final String OPERATION_PATH_REGEX = "/api/operations/[A-Za-z0-9_-]{20,64}";
    private static final String FORM_CONTENT_TYPE = "application/x-www-form-urlencoded; charset=UTF-8";
    private static final String ERROR_DIALOG_TITLE = "No se pudo abrir la demo";

    private static final int FIELD_COLUMNS = 26;
    private static final int MIN_TEXT_LENGTH = 1;
    private static final int MAX_TEXT_LENGTH = 100;
    private static final int TITLE_FONT_SIZE = 21;
    private static final float STATUS_FONT_SIZE = 12f;
    private static final Color STATUS_COLOR = new Color(80, 95, 115);
    private static final Duration CONNECT_TIMEOUT = Duration.ofSeconds(5);
    private static final Duration REQUEST_TIMEOUT = Duration.ofSeconds(20);

    private final URI operation;
    private final HttpClient client = HttpClient.newBuilder()
            .connectTimeout(CONNECT_TIMEOUT).build();
    private final JFrame window = new JFrame(WINDOW_TITLE);
    private final JTextField dateField = new JTextField(LocalDate.now().toString(), FIELD_COLUMNS);
    private final JTextField computerField = new JTextField(computerName(), FIELD_COLUMNS);
    private final JTextField aliasField = new JTextField(DEFAULT_ALIAS, FIELD_COLUMNS);
    private final JButton confirmButton = new JButton(CONFIRM_LABEL);
    private final JLabel status = new JLabel(INITIAL_STATUS);
    private boolean sending = false;

    private InformationForm(URI operation) {
        this.operation = operation;
        window.setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        JPanel content = new JPanel(new BorderLayout(0, 22));
        content.setBorder(BorderFactory.createEmptyBorder(26, 28, 24, 28));
        JLabel title = new JLabel(WINDOW_TITLE);
        title.setFont(title.getFont().deriveFont(Font.BOLD, TITLE_FONT_SIZE));
        content.add(title, BorderLayout.NORTH);

        JPanel fields = new JPanel(new GridBagLayout());
        addField(fields, DATE_LABEL, dateField, FIELD_DATE, 0, 'F');
        addField(fields, COMPUTER_NAME_LABEL, computerField, FIELD_COMPUTER_NAME, 1, 'N');
        addField(fields, ALIAS_LABEL, aliasField, FIELD_ALIAS, 2, 'A');
        content.add(fields, BorderLayout.CENTER);

        JPanel footer = new JPanel(new BorderLayout(0, 16));
        status.setFont(status.getFont().deriveFont(STATUS_FONT_SIZE));
        status.setForeground(STATUS_COLOR);
        footer.add(status, BorderLayout.NORTH);
        confirmButton.setName(CONFIRM_CONTROL_NAME);
        confirmButton.getAccessibleContext().setAccessibleName(CONFIRM_LABEL);
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
        String name = System.getenv(COMPUTER_NAME_ENV);
        if (name != null && !name.isBlank()) return name;
        try {
            return InetAddress.getLocalHost().getHostName();
        } catch (Exception error) {
            return DEFAULT_COMPUTER_NAME;
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
        values.put(FIELD_DATE, date);
        values.put(FIELD_COMPUTER_NAME, textValue(computerField, "nombre del equipo"));
        values.put(FIELD_ALIAS, textValue(aliasField, "alias"));
        return values;
    }

    private static String textValue(JTextField field, String name) {
        String value = field.getText().strip();
        int count = value.codePointCount(0, value.length());
        if (count < MIN_TEXT_LENGTH || count > MAX_TEXT_LENGTH || value.codePoints().anyMatch(Character::isISOControl)) {
            throw new IllegalArgumentException(
                    "El " + name + " debe contener entre " + MIN_TEXT_LENGTH + " y " + MAX_TEXT_LENGTH + " caracteres visibles.");
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
                post(RESULT_PATH_SUFFIX, data);
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
                post(CANCEL_PATH_SUFFIX, Map.of());
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
                .timeout(REQUEST_TIMEOUT)
                .header("Content-Type", FORM_CONTENT_TYPE)
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
            if (!ALLOWED_SCHEME.equals(operation.getScheme())
                    || !ALLOWED_HOSTS.contains(operation.getHost())
                    || operation.getRawQuery() != null || operation.getRawFragment() != null
                    || operation.getUserInfo() != null
                    || !operation.getPath().matches(OPERATION_PATH_REGEX)) {
                throw new IllegalArgumentException("La operación debe pertenecer al servidor local de la demo.");
            }
            UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
            SwingUtilities.invokeLater(() -> new InformationForm(operation));
        } catch (Exception error) {
            JOptionPane.showMessageDialog(null, error.getMessage(), ERROR_DIALOG_TITLE, JOptionPane.ERROR_MESSAGE);
            System.exit(1);
        }
    }
}
